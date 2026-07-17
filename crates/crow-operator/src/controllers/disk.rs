use std::{sync::Arc, time::Duration};

use futures::StreamExt;
use kube::api::{Api, ListParams, Patch, PatchParams};
use kube::ResourceExt;
use kube::{
    runtime::{
        controller::Action, finalizer, finalizer::Event as FinalizerEvent, watcher, Controller,
    },
    Client,
};
use sqlx::PgPool;
use uuid::Uuid;

use crow_core::{
    crd::resources::{Disk, DiskStatus},
    types::{ResourceHandle, VmHandle, VolumeHandle, VolumeSpec},
};
use crow_provider_registry::{resolve_provider_by_name, VM_NAMESPACE};

const FINALIZER: &str = "disk.crow.cloud/finalizer";

/// Reconciles `Disk` directly against `InfraProvider` (no `ResourceDriver`
/// layer) — unlike a VM's "provision once, poll forever" lifecycle, a
/// disk's whole job is reacting to `spec` changes (attach/detach/resize),
/// which is a diff against `status`, not a one-shot provision.
struct Ctx {
    client: Client,
    db: PgPool,
}

#[derive(Debug, thiserror::Error)]
enum ReconcileError {
    #[error("resource row not found for id {0}")]
    RowMissing(Uuid),
    #[error("CR name {0:?} is not a valid `disk-{{uuid}}` name")]
    BadCrName(String),
    #[error("disk {0:?} is marked attached but has no recorded volid")]
    Inconsistent(String),
    #[error(transparent)]
    Registry(#[from] crow_provider_registry::RegistryError),
    #[error(transparent)]
    Provider(#[from] crow_core::ProviderError),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Kube(#[from] kube::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub async fn run(client: Client, db: PgPool) -> anyhow::Result<()> {
    let api: Api<Disk> = Api::namespaced(client.clone(), VM_NAMESPACE);
    let ctx = Arc::new(Ctx { client, db });

    Controller::new(api, watcher::Config::default())
        .run(reconcile, error_policy, ctx)
        .for_each(|res| async move {
            if let Err(e) = res {
                tracing::warn!(error = %e, "disk reconcile failed");
            }
        })
        .await;
    Ok(())
}

fn resource_id_from_cr_name(cr_name: &str) -> Result<Uuid, ReconcileError> {
    cr_name
        .strip_prefix("disk-")
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| ReconcileError::BadCrName(cr_name.to_string()))
}

async fn reconcile(
    disk: Arc<Disk>,
    ctx: Arc<Ctx>,
) -> Result<Action, finalizer::Error<ReconcileError>> {
    let api: Api<Disk> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
    finalizer(&api, FINALIZER, disk, |event| async {
        match event {
            FinalizerEvent::Apply(disk) => apply(&disk, &ctx).await,
            FinalizerEvent::Cleanup(disk) => cleanup(&disk, &ctx).await,
        }
    })
    .await
}

fn error_policy(
    _disk: Arc<Disk>,
    _err: &finalizer::Error<ReconcileError>,
    _ctx: Arc<Ctx>,
) -> Action {
    Action::requeue(Duration::from_secs(30))
}

/// Destroys any real backing storage (if the disk was ever attached) and
/// removes the `resources` row. The API layer is expected to reject
/// deleting a disk that's still attached (see `crow-api`'s `routes::resources::remove`)
/// — this is a defense-in-depth backstop for CRs deleted out-of-band (e.g.
/// `kubectl delete`), not the primary safety check.
async fn cleanup(disk: &Disk, ctx: &Ctx) -> Result<Action, ReconcileError> {
    let name = disk.name_any();
    let resource_id = resource_id_from_cr_name(&name)?;

    if let Some(volid) = disk.status.as_ref().and_then(|s| s.volid.clone()) {
        let (_, infra) =
            resolve_provider_by_name(&ctx.db, &disk.spec.infra_provider_ref.name, &disk.spec.node)
                .await?;
        infra
            .delete_volume(&VolumeHandle {
                provider_type: "proxmox".to_string(),
                provider_id: volid,
            })
            .await?;
    }

    sqlx::query("DELETE FROM resources WHERE id = $1")
        .bind(resource_id)
        .execute(&ctx.db)
        .await?;

    Ok(Action::await_change())
}

/// A `Ready`, handle-bearing VM resource's `VmHandle`, or `None` if it
/// doesn't exist yet / isn't Ready / has no handle — callers should wait
/// rather than treat that as an error, mirroring how `virtual_machine.rs`
/// waits on an unbound `IpClaim`.
async fn fetch_ready_vm_handle(
    db: &PgPool,
    project: &str,
    vm_name: &str,
) -> Result<Option<VmHandle>, ReconcileError> {
    let row: Option<(String, Option<serde_json::Value>)> = sqlx::query_as(
        "SELECT phase, handle FROM resources
         WHERE project = $1 AND name = $2 AND resource_type = 'vm'",
    )
    .bind(project)
    .bind(vm_name)
    .fetch_optional(db)
    .await?;

    let Some((phase, Some(handle_json))) = row else {
        return Ok(None);
    };
    if phase != "Ready" {
        return Ok(None);
    }

    let resource_handle: ResourceHandle = serde_json::from_value(handle_json)?;
    let vm_handle: VmHandle = serde_json::from_value(resource_handle.data)?;
    Ok(Some(vm_handle))
}

async fn apply(disk: &Disk, ctx: &Ctx) -> Result<Action, ReconcileError> {
    let name = disk.name_any();
    let resource_id = resource_id_from_cr_name(&name)?;

    let (_, infra) =
        resolve_provider_by_name(&ctx.db, &disk.spec.infra_provider_ref.name, &disk.spec.node)
            .await?;

    let project: Option<(String,)> = sqlx::query_as("SELECT project FROM resources WHERE id = $1")
        .bind(resource_id)
        .fetch_optional(&ctx.db)
        .await?;
    let (project,) = project.ok_or(ReconcileError::RowMissing(resource_id))?;

    let status = disk.status.clone().unwrap_or_default();
    let wants_vm = disk.spec.vm_ref.clone();
    let attached_to = status.attached_vm_ref.clone();

    let (new_status, action) = match (&wants_vm, &attached_to) {
        // Attach.
        (Some(vm_ref), None) => {
            let Some(vm_handle) = fetch_ready_vm_handle(&ctx.db, &project, &vm_ref.name).await?
            else {
                return Ok(Action::requeue(Duration::from_secs(5)));
            };
            let volume = infra
                .attach_volume(
                    &vm_handle,
                    &VolumeSpec {
                        name: name.clone(),
                        size_gib: disk.spec.size_gib as u64,
                        storage_pool: None,
                    },
                )
                .await?;
            (
                DiskStatus {
                    phase: Some("Ready".to_string()),
                    volid: Some(volume.provider_id),
                    attached_size_gib: Some(disk.spec.size_gib),
                    attached_vm_ref: Some(vm_ref.clone()),
                    message: None,
                },
                Action::requeue(Duration::from_secs(300)),
            )
        }
        // Detach.
        (None, Some(attached_ref)) => {
            let volid = status
                .volid
                .clone()
                .ok_or_else(|| ReconcileError::Inconsistent(name.clone()))?;
            if let Some(vm_handle) =
                fetch_ready_vm_handle(&ctx.db, &project, &attached_ref.name).await?
            {
                infra
                    .detach_volume(
                        &vm_handle,
                        &VolumeHandle {
                            provider_type: "proxmox".to_string(),
                            provider_id: volid.clone(),
                        },
                    )
                    .await?;
            }
            // If the VM is already gone entirely, there's nothing left to
            // detach from — just record the disk as unattached either way.
            (
                DiskStatus {
                    phase: Some("Ready".to_string()),
                    volid: Some(volid),
                    attached_size_gib: status.attached_size_gib,
                    attached_vm_ref: None,
                    message: None,
                },
                Action::requeue(Duration::from_secs(300)),
            )
        }
        // Moving an attached disk directly to a different VM without
        // detaching first isn't supported — surface it rather than attempt it.
        (Some(a), Some(b)) if a.name != b.name => (
            DiskStatus {
                phase: Some("Failed".to_string()),
                volid: status.volid.clone(),
                attached_size_gib: status.attached_size_gib,
                attached_vm_ref: status.attached_vm_ref.clone(),
                message: Some(
                    "moving an attached disk directly to a different VM is not supported — \
                     detach first"
                        .to_string(),
                ),
            },
            Action::requeue(Duration::from_secs(60)),
        ),
        // Steady state: already attached to the right VM (check resize), or
        // already unattached (just mirror the declared size in status).
        _ => {
            if let Some(attached_ref) = &attached_to {
                let current_size = status.attached_size_gib.unwrap_or(disk.spec.size_gib);
                if disk.spec.size_gib > current_size {
                    let volid = status
                        .volid
                        .clone()
                        .ok_or_else(|| ReconcileError::Inconsistent(name.clone()))?;
                    if let Some(vm_handle) =
                        fetch_ready_vm_handle(&ctx.db, &project, &attached_ref.name).await?
                    {
                        infra
                            .resize_volume(
                                &vm_handle,
                                &VolumeHandle {
                                    provider_type: "proxmox".to_string(),
                                    provider_id: volid,
                                },
                                disk.spec.size_gib as u64,
                            )
                            .await?;
                    }
                }
                (
                    DiskStatus {
                        phase: Some("Ready".to_string()),
                        volid: status.volid.clone(),
                        attached_size_gib: Some(disk.spec.size_gib.max(current_size)),
                        attached_vm_ref: Some(attached_ref.clone()),
                        message: None,
                    },
                    Action::requeue(Duration::from_secs(300)),
                )
            } else {
                (
                    DiskStatus {
                        phase: Some("Ready".to_string()),
                        volid: None,
                        attached_size_gib: Some(disk.spec.size_gib),
                        attached_vm_ref: None,
                        message: None,
                    },
                    Action::requeue(Duration::from_secs(300)),
                )
            }
        }
    };

    let api: Api<Disk> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
    api.patch_status(
        &name,
        &PatchParams::default(),
        &Patch::Merge(serde_json::json!({ "status": new_status })),
    )
    .await?;

    // Mirror into the generic `resources` row so the existing project-scoped
    // list/detail API (and the frontend that already consumes it for VMs)
    // works for disks with no new endpoints needed.
    let handle = serde_json::json!({
        "resource_type": "Disk",
        "data": {
            "size_gib": disk.spec.size_gib,
            "attached_size_gib": new_status.attached_size_gib,
            "volid": new_status.volid,
            "attached_vm_ref": new_status.attached_vm_ref,
        }
    });
    sqlx::query("UPDATE resources SET phase = $1, handle = $2, updated_at = NOW() WHERE id = $3")
        .bind(new_status.phase.as_deref().unwrap_or("Pending"))
        .bind(&handle)
        .bind(resource_id)
        .execute(&ctx.db)
        .await?;

    Ok(action)
}

/// Disk CRs currently (per `status`, not `spec`) attached to the VM named
/// `vm_name` within `project` — used by `virtual_machine.rs`'s cleanup to
/// detach real storage before a VM purge would otherwise destroy it.
/// Filters by the disk's own project (a second Postgres lookup per
/// candidate) since `ResourceRef.name` alone isn't globally unique — only
/// unique within a project.
pub async fn list_attached_to(
    client: &Client,
    db: &PgPool,
    project: &str,
    vm_name: &str,
) -> anyhow::Result<Vec<Disk>> {
    let api: Api<Disk> = Api::namespaced(client.clone(), VM_NAMESPACE);
    let all = api.list(&ListParams::default()).await?;

    let mut matched = Vec::new();
    for disk in all.items {
        let Some(attached_ref) = disk
            .status
            .as_ref()
            .and_then(|s| s.attached_vm_ref.as_ref())
        else {
            continue;
        };
        if attached_ref.name != vm_name {
            continue;
        }
        let Ok(resource_id) = resource_id_from_cr_name(&disk.name_any()) else {
            continue;
        };
        let disk_project: Option<(String,)> =
            sqlx::query_as("SELECT project FROM resources WHERE id = $1")
                .bind(resource_id)
                .fetch_optional(db)
                .await?;
        if disk_project.map(|(p,)| p) == Some(project.to_string()) {
            matched.push(disk);
        }
    }
    Ok(matched)
}

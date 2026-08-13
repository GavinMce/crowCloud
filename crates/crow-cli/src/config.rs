#![allow(dead_code)]

use crate::iso::fabric::FabricConfig;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Config {
    pub server: Option<String>,
    pub token: Option<String>,
    pub current_project: Option<String>,
    /// Cached locally so `crow-cli iso proxmox build` works before any
    /// crowCloud instance exists (the seed-election bootstrap case,
    /// #67) -- generated once on first use, then reused for every
    /// subsequent build so images keep authenticating against the same
    /// fleet. Not fetched from the server; this is the client-side half
    /// of the same value a `fleet_secrets` row on the server holds once
    /// crowCloud is up (see `iso::proxmox`).
    pub fleet_secret: Option<String>,
    /// Set once via `crow iso fabric-configure`, read automatically by
    /// every ISO build command afterwards (see `iso::fabric`).
    pub fabric: Option<FabricConfig>,
    /// VyOS's own WireGuard server private key -- generated once on
    /// first `iso vyos build` with WireGuard enabled, then reused for
    /// every subsequent build/rebuild so re-running it doesn't mint a
    /// new server key and silently invalidate every admin's already-
    /// distributed client `.conf` (same reasoning as `fleet_secret`
    /// above, applied to a different secret).
    pub wireguard_server_private_key: Option<String>,
}

impl Config {
    pub fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("crow")
            .join("config.json")
    }

    pub fn load() -> Result<Self> {
        let path = Self::path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path();
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Returns the cached fleet secret, generating and persisting one on
    /// first use. A fresh, random secret generated purely locally --
    /// deliberately no network/login dependency, since the very first
    /// image built for a brand-new fleet has no crowCloud instance to
    /// ask yet (#67).
    pub fn fleet_secret_or_generate() -> Result<String> {
        let mut cfg = Self::load()?;
        if let Some(secret) = &cfg.fleet_secret {
            return Ok(secret.clone());
        }
        let secret = uuid::Uuid::new_v4().simple().to_string();
        cfg.fleet_secret = Some(secret.clone());
        cfg.save()?;
        Ok(secret)
    }

    /// Returns VyOS's cached WireGuard server private key, generating
    /// one (via `wg genkey`) on first use. Reused on every subsequent
    /// `iso vyos build` for the same reason `fleet_secret_or_generate`
    /// caches its own value -- see this field's own doc comment.
    pub fn wireguard_server_key_or_generate() -> Result<String> {
        let mut cfg = Self::load()?;
        if let Some(key) = &cfg.wireguard_server_private_key {
            return Ok(key.clone());
        }
        let key = crate::iso::wireguard::genkey()
            .context("generating VyOS's WireGuard server private key")?;
        cfg.wireguard_server_private_key = Some(key.clone());
        cfg.save()?;
        Ok(key)
    }
}

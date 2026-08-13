/// Shared bash preamble for scripts meant to be copied onto a box and run
/// by hand over SSH (the Proxmox post-install hook and the VyOS
/// fabric-init script) rather than fired automatically at boot.
///
/// Neither `--on-first-boot` (Proxmox) nor a `cron.d` `@reboot` entry
/// (VyOS) reliably triggered in practice -- both scripts are now delivered
/// as a manual step instead (copy the file over, run it, watch it). That
/// makes plain `echo "==> ..."` headers with no failure context the wrong
/// shape: a script someone is actively watching over SSH should say
/// exactly which numbered step it's on, print the full completed-step
/// list the moment anything fails (so a partial run is diagnosable without
/// scrolling back through the whole log), and confirm explicitly when
/// everything succeeded.
///
/// `step()` doesn't take a pre-known total step count -- both scripts
/// branch (Proxmox's seed-election vs. join-existing-cluster paths), so
/// the number of steps that actually run isn't fixed at render time.
/// Counting up as each step starts, rather than "N of M", avoids
/// advertising a total that a given run will never reach.
use std::fmt::Write as _;

/// `step`/`fail`/`on_error`/`on_success` plus the `ERR` trap. Callers
/// interpolate this once, right after `set -euo pipefail`, then call
/// `step "description"` before each major phase and `on_success` as the
/// literal last line of the script (only reached if nothing earlier
/// called `fail` or hit the `ERR` trap).
pub fn render_step_framework() -> String {
    r#"STEP_NUM=0
STEP_LOG=()
CURRENT_STEP="(startup)"

step() {
    STEP_NUM=$((STEP_NUM + 1))
    CURRENT_STEP="$1"
    STEP_LOG+=("$1")
    echo ""
    echo "=== [${STEP_NUM}] ${CURRENT_STEP} ==="
}

print_completed_steps() {
    if [ "${#STEP_LOG[@]}" -eq 0 ]; then
        echo "  (no steps completed)"
        return
    fi
    local i
    for i in "${!STEP_LOG[@]}"; do
        echo "  $((i + 1)). ${STEP_LOG[$i]}"
    done
}

# For explicit failure paths (a check that fails cleanly, not a command
# that errors out) -- prints the same completed-step context an ERR trap
# would, so every exit-1 path in the script is equally diagnosable.
fail() {
    echo "" >&2
    echo "!!! ${1}" >&2
    echo "!!! Failed during step [${STEP_NUM}] \"${CURRENT_STEP}\"" >&2
    echo "" >&2
    echo "Steps completed before this failure:" >&2
    print_completed_steps >&2
    exit 1
}

on_error() {
    local exit_code=$? line=$1
    echo "" >&2
    echo "!!! Command failed (line ${line}, exit ${exit_code}): ${BASH_COMMAND}" >&2
    echo "!!! Failed during step [${STEP_NUM}] \"${CURRENT_STEP}\"" >&2
    echo "" >&2
    echo "Steps completed before this failure:" >&2
    print_completed_steps >&2
    exit "${exit_code}"
}
trap 'on_error ${LINENO}' ERR

on_success() {
    echo ""
    echo "=== All ${STEP_NUM} steps completed successfully ==="
    print_completed_steps
}
"#
    .to_string()
}

/// `on_success` called as the script's own last line -- separate from
/// `render_step_framework` since callers interpolate it at the opposite
/// end of the script.
pub fn render_on_success_call() -> String {
    let mut out = String::new();
    let _ = writeln!(out, "on_success");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framework_traps_err_and_defines_step_fail_and_success_hooks() {
        let out = render_step_framework();
        assert!(out.contains("trap 'on_error ${LINENO}' ERR"));
        assert!(out.contains("step() {"));
        assert!(out.contains("fail() {"));
        assert!(out.contains("on_error() {"));
        assert!(out.contains("on_success() {"));
    }

    #[test]
    fn fail_and_on_error_both_print_completed_steps_before_exiting() {
        let out = render_step_framework();
        let fail_body = out.split("fail() {").nth(1).unwrap();
        let fail_body = &fail_body[..fail_body.find("on_error() {").unwrap()];
        assert!(fail_body.contains("print_completed_steps >&2"));
        assert!(fail_body.contains("exit 1"));

        let error_body = out.split("on_error() {").nth(1).unwrap();
        let error_body = &error_body[..error_body.find("on_success() {").unwrap()];
        assert!(error_body.contains("print_completed_steps >&2"));
        assert!(error_body.contains(r#"exit "${exit_code}""#));
    }
}

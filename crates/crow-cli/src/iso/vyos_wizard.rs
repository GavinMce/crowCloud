/// Interactive prompting for `iso vyos build`/`iso vyos flavor` (#66) --
/// walking through every field one at a time, like an installer wizard,
/// instead of requiring ~25 flags on one command line up front.
///
/// Every flag stays usable for scripting/CI: any field supplied on the
/// command line is used as-is with no prompt; only omitted fields are
/// asked for. Running with no flags at all is therefore a full wizard;
/// running with all flags is unchanged from before this existed.
///
/// Confirmed live: a naive "prompt if the flag's absent" rule breaks a
/// fully-flagged, non-interactive invocation the moment ANY field is
/// left to its old implicit default (e.g. not passing `--dns-servers`,
/// which used to fall back to a clap `default_values_t`) -- it tries to
/// open a terminal that isn't there and dies on a cryptic "not a
/// terminal" IO error instead of just using the default like it did
/// before interactive mode existed. Every helper here checks
/// `is_interactive()` first and falls back to `default` (or errors
/// clearly, for fields with no sensible default) instead of ever
/// attempting to prompt in a non-interactive context.
use anyhow::{bail, Context, Result};
use dialoguer::{Confirm, Input, Password};
use std::io::IsTerminal;
use std::path::PathBuf;

fn is_interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// Prompts for any `FromStr`/`Display` value if `flag_value` is `None`,
/// otherwise returns it unchanged -- the single mechanism every other
/// prompt helper here is built on.
pub fn prompt<T>(flag_value: Option<T>, label: &str, default: Option<T>) -> Result<T>
where
    T: Clone + std::fmt::Display + std::str::FromStr,
    T::Err: std::fmt::Display + std::marker::Send + std::marker::Sync + 'static,
{
    if let Some(v) = flag_value {
        return Ok(v);
    }
    if !is_interactive() {
        return match default {
            Some(d) => Ok(d),
            None => bail!("'{label}' was not provided and no terminal is available to prompt for it -- pass it explicitly via its flag"),
        };
    }
    let mut input = Input::<T>::new().with_prompt(label);
    if let Some(d) = default {
        input = input.default(d);
    }
    input.interact_text().context("reading interactive input")
}

/// Same as `prompt`, but for a field that's allowed to stay empty
/// (mapped to `None`) -- an empty response is accepted as-is rather
/// than re-prompting. Unlike `prompt`, there's no "flag not given"
/// vs. "field intentionally absent" distinction to make here: clap
/// already collapses those to the same `None` for a plain optional
/// flag, so `flag_value` is a normal `Option<String>`, not a
/// double-`Option`.
pub fn prompt_optional(flag_value: Option<String>, label: &str) -> Result<Option<String>> {
    if let Some(v) = flag_value {
        return Ok(Some(v));
    }
    if !is_interactive() {
        return Ok(None);
    }
    let response: String = Input::new()
        .with_prompt(format!("{label} (optional, leave blank to skip)"))
        .allow_empty(true)
        .interact_text()
        .context("reading interactive input")?;
    Ok(if response.trim().is_empty() {
        None
    } else {
        Some(response.trim().to_string())
    })
}

/// `PathBuf` doesn't implement `Display`, so it can't go through the
/// generic `prompt`/`FromStr` path -- prompted as a plain string instead.
pub fn prompt_path(flag_value: Option<PathBuf>, label: &str) -> Result<PathBuf> {
    if let Some(v) = flag_value {
        return Ok(v);
    }
    if !is_interactive() {
        bail!("'{label}' was not provided and no terminal is available to prompt for it -- pass it explicitly via its flag");
    }
    let response: String = Input::new()
        .with_prompt(label)
        .interact_text()
        .context("reading interactive input")?;
    Ok(PathBuf::from(response))
}

/// Masked input (not echoed to the terminal/scrollback) -- for shared
/// secrets like the BGP peer-group password.
pub fn prompt_secret(flag_value: Option<String>, label: &str) -> Result<String> {
    if let Some(v) = flag_value {
        return Ok(v);
    }
    if !is_interactive() {
        bail!("'{label}' was not provided and no terminal is available to prompt for it -- pass it explicitly via its flag");
    }
    Password::new()
        .with_prompt(label)
        .interact()
        .context("reading interactive password input")
}

pub fn prompt_bool(flag_value: Option<bool>, label: &str, default: bool) -> Result<bool> {
    if let Some(v) = flag_value {
        return Ok(v);
    }
    if !is_interactive() {
        return Ok(default);
    }
    Confirm::new()
        .with_prompt(label)
        .default(default)
        .interact()
        .context("reading interactive confirmation")
}

/// Comma-separated list, prompted once as a single line and split --
/// used for `--dns-servers` (has a sensible default, mainly "press
/// enter to accept") and `--disk` (no sensible default -- `None`
/// errors clearly in a non-interactive context instead of silently
/// building with an empty disk list).
pub fn prompt_list(
    flag_value: Option<Vec<String>>,
    label: &str,
    default: Option<&[String]>,
) -> Result<Vec<String>> {
    if let Some(v) = flag_value {
        return Ok(v);
    }
    if !is_interactive() {
        return match default {
            Some(d) => Ok(d.to_vec()),
            None => bail!("'{label}' was not provided and no terminal is available to prompt for it -- pass it explicitly via its flag"),
        };
    }
    let mut input = Input::<String>::new().with_prompt(label);
    if let Some(d) = default {
        input = input.default(d.join(","));
    }
    let response: String = input.interact_text().context("reading interactive input")?;
    Ok(response
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_returns_the_flag_value_without_touching_the_terminal_when_present() {
        // The whole point: a fully-flagged invocation (CI/scripting)
        // must never block on stdin. This is the only branch testable
        // without a real terminal -- the prompting branch is exercised
        // by hand, not in CI.
        assert_eq!(prompt(Some(9000u32), "trunk mtu", None).unwrap(), 9000);
        assert_eq!(
            prompt(Some("eth1".to_string()), "trunk interface", None).unwrap(),
            "eth1"
        );
    }

    #[test]
    fn prompt_falls_back_to_the_default_instead_of_erroring_when_non_interactive() {
        // Confirmed live: cargo test has no terminal attached, so this
        // exercises the exact non-interactive path a CI/scripted
        // invocation hits when a field with a sensible default (e.g.
        // trunk_mtu) is omitted.
        assert_eq!(prompt(None, "trunk mtu", Some(9000u32)).unwrap(), 9000);
    }

    #[test]
    fn prompt_errors_clearly_instead_of_hanging_when_non_interactive_with_no_default() {
        let err = prompt::<String>(None, "hostname", None).unwrap_err();
        assert!(err.to_string().contains("hostname"));
        assert!(err.to_string().contains("no terminal is available"));
    }

    #[test]
    fn prompt_bool_returns_the_flag_value_without_touching_the_terminal_when_present() {
        assert!(prompt_bool(Some(true), "use dhcp", false).unwrap());
        assert!(!prompt_bool(Some(false), "use dhcp", true).unwrap());
    }

    #[test]
    fn prompt_bool_falls_back_to_the_default_when_non_interactive() {
        assert!(!prompt_bool(None, "pin trunk speed", false).unwrap());
        assert!(prompt_bool(None, "pin trunk speed", true).unwrap());
    }

    #[test]
    fn prompt_list_returns_the_flag_value_without_touching_the_terminal_when_present() {
        let v = vec!["1.1.1.1".to_string(), "9.9.9.9".to_string()];
        assert_eq!(
            prompt_list(Some(v.clone()), "dns servers", None).unwrap(),
            v
        );
    }

    #[test]
    fn prompt_list_falls_back_to_the_default_when_non_interactive() {
        let default = vec!["8.8.8.8".to_string(), "8.8.4.4".to_string()];
        assert_eq!(
            prompt_list(None, "dns servers", Some(&default)).unwrap(),
            default
        );
    }

    #[test]
    fn prompt_list_errors_clearly_when_non_interactive_with_no_default() {
        let err = prompt_list(None, "disk", None).unwrap_err();
        assert!(err.to_string().contains("disk"));
    }

    #[test]
    fn prompt_path_returns_the_flag_value_without_touching_the_terminal_when_present() {
        assert_eq!(
            prompt_path(Some(PathBuf::from("/tmp/key")), "ssh key path").unwrap(),
            PathBuf::from("/tmp/key")
        );
    }

    #[test]
    fn prompt_path_errors_clearly_when_non_interactive() {
        let err = prompt_path(None, "ssh key path").unwrap_err();
        assert!(err.to_string().contains("ssh key path"));
    }

    #[test]
    fn prompt_secret_returns_the_flag_value_without_touching_the_terminal_when_present() {
        assert_eq!(
            prompt_secret(Some("shh".to_string()), "bgp password").unwrap(),
            "shh"
        );
    }

    #[test]
    fn prompt_secret_errors_clearly_when_non_interactive() {
        let err = prompt_secret(None, "bgp password").unwrap_err();
        assert!(err.to_string().contains("bgp password"));
    }

    #[test]
    fn prompt_optional_returns_the_flag_value_without_touching_the_terminal_when_present() {
        assert_eq!(
            prompt_optional(Some("gw".to_string()), "gateway").unwrap(),
            Some("gw".to_string())
        );
    }

    #[test]
    fn prompt_optional_falls_back_to_none_when_non_interactive() {
        assert_eq!(prompt_optional(None, "gateway").unwrap(), None);
    }
}

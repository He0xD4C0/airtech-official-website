#[cfg(test)]
mod runtime_tests {
    use super::{is_production_marker, is_true_marker, Cli, CmsAction, Command};
    use clap::Parser;

    #[test]
    fn production_runtime_markers_are_strict_and_case_insensitive() {
        for value in ["production", "PROD", " live "] {
            assert!(is_production_marker(value));
        }
        for value in ["development", "test", "staging", ""] {
            assert!(!is_production_marker(value));
        }
        for value in ["1", "TRUE", " yes ", "on"] {
            assert!(is_true_marker(value));
        }
        for value in ["0", "false", "off", ""] {
            assert!(!is_true_marker(value));
        }
    }

    #[test]
    fn parses_cms_migration_preflight_command() {
        let cli = Cli::try_parse_from(["airtekctl", "cms", "preflight"])
            .expect("CMS preflight command should parse");

        assert!(matches!(
            cli.command,
            Command::Cms {
                action: CmsAction::Preflight
            }
        ));
    }
}

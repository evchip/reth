//! clap [Args](clap::Args) for database configuration

use std::time::Duration;

use crate::version::default_client_version;
use clap::{
    builder::{PossibleValue, TypedValueParser},
    error::ErrorKind,
    Arg, Args, Command, Error,
};
use reth_db::{mdbx::MaxReadTransactionDuration, ClientVersion};
use reth_node_types::ByteSize;
use reth_storage_errors::db::LogLevel;

/// Parameters for database configuration
#[derive(Debug, Args, PartialEq, Eq, Default, Clone, Copy)]
#[command(next_help_heading = "Database")]
pub struct DatabaseArgs {
    /// Database logging level. Levels higher than "notice" require a debug build.
    #[arg(long = "db.log-level", value_parser = LogLevelValueParser::default())]
    pub log_level: Option<LogLevel>,
    /// Open environment in exclusive/monopolistic mode. Makes it possible to open a database on an
    /// NFS volume.
    #[arg(long = "db.exclusive")]
    pub exclusive: Option<bool>,
    /// Maximum database size (e.g., 4TB, 8MB)
    #[arg(long = "db.max-size", value_parser = parse_byte_size)]
    pub max_size: Option<ByteSize>,
    /// Database growth step (e.g., 4GB, 4KB)
    #[arg(long = "db.growth-step", value_parser = parse_byte_size)]
    pub growth_step: Option<ByteSize>,
    /// Read transaction timeout in seconds, 0 means no timeout.
    #[arg(long = "db.read-transaction-timeout")]
    pub read_transaction_timeout: Option<u64>,
}

impl DatabaseArgs {
    /// Returns default database arguments with configured log level and client version.
    pub fn database_args(&self) -> reth_db::mdbx::DatabaseArguments {
        self.get_database_args(default_client_version())
    }

    /// Returns the database arguments with configured log level, client version,
    /// max read transaction duration, and geometry.
    pub fn get_database_args(
        &self,
        client_version: ClientVersion,
    ) -> reth_db::mdbx::DatabaseArguments {
        let max_read_transaction_duration = match self.read_transaction_timeout {
            None => None, // if not specified, use default value
            Some(0) => Some(MaxReadTransactionDuration::Unbounded), // if 0, disable timeout
            Some(secs) => Some(MaxReadTransactionDuration::Set(Duration::from_secs(secs))),
        };

        reth_db::mdbx::DatabaseArguments::new(client_version)
            .with_log_level(self.log_level)
            .with_exclusive(self.exclusive)
            .with_max_read_transaction_duration(max_read_transaction_duration)
            .with_geometry(self.max_size.map(|s| s.0), self.growth_step.map(|s| s.0))
    }
}

/// clap value parser for [`LogLevel`].
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
struct LogLevelValueParser;

impl TypedValueParser for LogLevelValueParser {
    type Value = LogLevel;

    fn parse_ref(
        &self,
        _cmd: &Command,
        arg: Option<&Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, Error> {
        let val =
            value.to_str().ok_or_else(|| Error::raw(ErrorKind::InvalidUtf8, "Invalid UTF-8"))?;

        val.parse::<LogLevel>().map_err(|err| {
            let arg = arg.map(|a| a.to_string()).unwrap_or_else(|| "...".to_owned());
            let possible_values = LogLevel::value_variants()
                .iter()
                .map(|v| format!("- {:?}: {}", v, v.help_message()))
                .collect::<Vec<_>>()
                .join("\n");
            let msg = format!(
                "Invalid value '{val}' for {arg}: {err}.\n    Possible values:\n{possible_values}"
            );
            clap::Error::raw(clap::error::ErrorKind::InvalidValue, msg)
        })
    }

    fn possible_values(&self) -> Option<Box<dyn Iterator<Item = PossibleValue> + '_>> {
        let values = LogLevel::value_variants()
            .iter()
            .map(|v| PossibleValue::new(v.variant_name()).help(v.help_message()));
        Some(Box::new(values))
    }
}

fn parse_byte_size(s: &str) -> Result<ByteSize, String> {
    s.parse()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use reth_db::mdbx::{GIGABYTE, KILOBYTE, MEGABYTE, TERABYTE};

    /// A helper type to parse Args more easily
    #[derive(Parser)]
    struct CommandParser<T: Args> {
        #[command(flatten)]
        args: T,
    }

    #[test]
    fn test_default_database_args() {
        let default_args = DatabaseArgs::default();
        let args = CommandParser::<DatabaseArgs>::parse_from(["reth"]).args;
        assert_eq!(args, default_args);
    }

    #[test]
    fn test_command_parser_with_valid_max_size() {
        let cmd = CommandParser::<DatabaseArgs>::try_parse_from([
            "reth",
            "--db.max-size",
            "4398046511104",
        ])
        .unwrap();
        assert_eq!(cmd.args.max_size, Some(ByteSize(TERABYTE * 4)));
    }

    #[test]
    fn test_command_parser_with_invalid_max_size() {
        let result =
            CommandParser::<DatabaseArgs>::try_parse_from(["reth", "--db.max-size", "invalid"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_command_parser_with_valid_growth_step() {
        let cmd = CommandParser::<DatabaseArgs>::try_parse_from([
            "reth",
            "--db.growth-step",
            "4294967296",
        ])
        .unwrap();
        assert_eq!(cmd.args.growth_step, Some(ByteSize(GIGABYTE * 4)));
    }

    #[test]
    fn test_command_parser_with_invalid_growth_step() {
        let result =
            CommandParser::<DatabaseArgs>::try_parse_from(["reth", "--db.growth-step", "invalid"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_command_parser_with_valid_max_size_and_growth_step_from_str() {
        let cmd = CommandParser::<DatabaseArgs>::try_parse_from([
            "reth",
            "--db.max-size",
            "2TB",
            "--db.growth-step",
            "1GB",
        ])
        .unwrap();
        assert_eq!(cmd.args.max_size, Some(ByteSize(TERABYTE * 2)));
        assert_eq!(cmd.args.growth_step, Some(ByteSize(GIGABYTE)));

        let cmd = CommandParser::<DatabaseArgs>::try_parse_from([
            "reth",
            "--db.max-size",
            "12MB",
            "--db.growth-step",
            "2KB",
        ])
        .unwrap();
        assert_eq!(cmd.args.max_size, Some(ByteSize(MEGABYTE * 12)));
        assert_eq!(cmd.args.growth_step, Some(ByteSize(KILOBYTE * 2)));

        // with spaces
        let cmd = CommandParser::<DatabaseArgs>::try_parse_from([
            "reth",
            "--db.max-size",
            "12 MB",
            "--db.growth-step",
            "2 KB",
        ])
        .unwrap();
        assert_eq!(cmd.args.max_size, Some(ByteSize(MEGABYTE * 12)));
        assert_eq!(cmd.args.growth_step, Some(ByteSize(KILOBYTE * 2)));

        let cmd = CommandParser::<DatabaseArgs>::try_parse_from([
            "reth",
            "--db.max-size",
            "1073741824",
            "--db.growth-step",
            "1048576",
        ])
        .unwrap();
        assert_eq!(cmd.args.max_size, Some(ByteSize(GIGABYTE)));
        assert_eq!(cmd.args.growth_step, Some(ByteSize(MEGABYTE)));
    }

    #[test]
    fn test_command_parser_max_size_and_growth_step_from_str_invalid_unit() {
        let result =
            CommandParser::<DatabaseArgs>::try_parse_from(["reth", "--db.growth-step", "1 PB"]);
        assert!(result.is_err());

        let result =
            CommandParser::<DatabaseArgs>::try_parse_from(["reth", "--db.max-size", "2PB"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_possible_values() {
        // Initialize the LogLevelValueParser
        let parser = LogLevelValueParser;

        // Call the possible_values method
        let possible_values: Vec<PossibleValue> = parser.possible_values().unwrap().collect();

        // Expected possible values
        let expected_values = vec![
            PossibleValue::new("fatal")
                .help("Enables logging for critical conditions, i.e. assertion failures"),
            PossibleValue::new("error").help("Enables logging for error conditions"),
            PossibleValue::new("warn").help("Enables logging for warning conditions"),
            PossibleValue::new("notice")
                .help("Enables logging for normal but significant condition"),
            PossibleValue::new("verbose").help("Enables logging for verbose informational"),
            PossibleValue::new("debug").help("Enables logging for debug-level messages"),
            PossibleValue::new("trace").help("Enables logging for trace debug-level messages"),
            PossibleValue::new("extra").help("Enables logging for extra debug-level messages"),
        ];

        // Check that the possible values match the expected values
        assert_eq!(possible_values.len(), expected_values.len());
        for (actual, expected) in possible_values.iter().zip(expected_values.iter()) {
            assert_eq!(actual.get_name(), expected.get_name());
            assert_eq!(actual.get_help(), expected.get_help());
        }
    }

    #[test]
    fn test_command_parser_with_valid_log_level() {
        let cmd =
            CommandParser::<DatabaseArgs>::try_parse_from(["reth", "--db.log-level", "Debug"])
                .unwrap();
        assert_eq!(cmd.args.log_level, Some(LogLevel::Debug));
    }

    #[test]
    fn test_command_parser_with_invalid_log_level() {
        let result =
            CommandParser::<DatabaseArgs>::try_parse_from(["reth", "--db.log-level", "invalid"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_command_parser_without_log_level() {
        let cmd = CommandParser::<DatabaseArgs>::try_parse_from(["reth"]).unwrap();
        assert_eq!(cmd.args.log_level, None);
    }
}

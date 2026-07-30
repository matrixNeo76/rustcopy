//! Interpretation of robocopy's bitmask exit codes.
//!
//! Robocopy returns a bitmask: values 0-7 are success/informational, values >= 8 signal that at
//! least one item could not be copied. See the "Exit codes" section of the Microsoft Learn
//! robocopy reference: <https://learn.microsoft.com/windows-server/administration/windows-commands/robocopy>

/// Bit 0 (1): one or more files were copied successfully.
pub const BIT_COPIED: i32 = 0x01;
/// Bit 1 (2): extra files or directories were detected in the destination.
pub const BIT_EXTRA: i32 = 0x02;
/// Bit 2 (4): mismatched files or directories were detected.
pub const BIT_MISMATCH: i32 = 0x04;
/// Bit 3 (8): some files or directories could not be copied (retry limit exceeded).
pub const BIT_COPY_ERRORS: i32 = 0x08;
/// Bit 4 (16): serious error; robocopy did not copy any file.
pub const BIT_FATAL: i32 = 0x10;

/// Structured view over a robocopy exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RobocopyStatus {
    pub raw: i32,
}

impl RobocopyStatus {
    pub const fn new(raw: i32) -> Self {
        Self { raw }
    }

    /// True when the process was terminated by a signal or returned a negative code.
    /// Not reachable on Windows, but keeps the classification total.
    pub const fn is_abnormal(self) -> bool {
        self.raw < 0
    }

    fn has(self, bit: i32) -> bool {
        !self.is_abnormal() && (self.raw & bit) != 0
    }

    /// At least one file was copied.
    pub fn files_copied(self) -> bool {
        self.has(BIT_COPIED)
    }

    /// Extra files/dirs present in the destination.
    pub fn extra_detected(self) -> bool {
        self.has(BIT_EXTRA)
    }

    /// Mismatched files/dirs detected.
    pub fn mismatch_detected(self) -> bool {
        self.has(BIT_MISMATCH)
    }

    /// Some items could not be copied: robocopy already exhausted its own /R retries.
    pub fn copy_errors(self) -> bool {
        self.has(BIT_COPY_ERRORS)
    }

    /// Serious error: robocopy copied nothing (usage error, invalid path, access denied on root).
    pub fn fatal(self) -> bool {
        self.has(BIT_FATAL)
    }

    /// Nothing to do: source and destination were already in sync.
    pub fn no_change(self) -> bool {
        self.raw == 0
    }

    /// Codes 0-7 mean the run is acceptable; 8 and above mean at least one item failed.
    pub fn is_success(self) -> bool {
        !self.is_abnormal() && self.raw < BIT_COPY_ERRORS
    }

    /// Retrying is worthwhile for transient I/O failures (bit 3) but pointless for fatal
    /// errors (bit 4), which indicate a configuration problem rather than a flaky file.
    pub fn should_retry(self) -> bool {
        if self.is_abnormal() {
            return true;
        }
        self.copy_errors() && !self.fatal()
    }

    /// Human readable summary used in logs and in the JSON report.
    pub fn describe(self) -> String {
        if self.is_abnormal() {
            return format!("abnormal termination (raw {})", self.raw);
        }
        if self.no_change() {
            return "no files copied, source and destination already in sync".to_string();
        }

        let mut parts: Vec<&str> = Vec::new();
        if self.files_copied() {
            parts.push("files copied");
        }
        if self.extra_detected() {
            parts.push("extra files or directories detected");
        }
        if self.mismatch_detected() {
            parts.push("mismatched files or directories detected");
        }
        if self.copy_errors() {
            parts.push("some items could not be copied");
        }
        if self.fatal() {
            parts.push("fatal error, no files copied");
        }

        let unknown_bits =
            self.raw & !(BIT_COPIED | BIT_EXTRA | BIT_MISMATCH | BIT_COPY_ERRORS | BIT_FATAL);
        if unknown_bits != 0 {
            return if parts.is_empty() {
                format!("unrecognised exit code {}", self.raw)
            } else {
                format!(
                    "{} (plus unrecognised bits {:#x})",
                    parts.join("; "),
                    unknown_bits
                )
            };
        }

        parts.join("; ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_means_nothing_to_do() {
        let s = RobocopyStatus::new(0);
        assert!(s.is_success());
        assert!(s.no_change());
        assert!(!s.should_retry());
        assert!(!s.files_copied());
        assert_eq!(
            s.describe(),
            "no files copied, source and destination already in sync"
        );
    }

    #[test]
    fn codes_one_to_seven_are_success() {
        for code in 1..=7 {
            let s = RobocopyStatus::new(code);
            assert!(s.is_success(), "code {code} should be success");
            assert!(!s.should_retry(), "code {code} should not retry");
        }
    }

    #[test]
    fn codes_eight_and_above_are_failures() {
        for code in [8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 24, 31] {
            assert!(
                !RobocopyStatus::new(code).is_success(),
                "code {code} should fail"
            );
        }
    }

    #[test]
    fn bit_decoding_is_correct() {
        let s = RobocopyStatus::new(1);
        assert!(s.files_copied() && !s.extra_detected() && !s.mismatch_detected());

        let s = RobocopyStatus::new(3);
        assert!(s.files_copied() && s.extra_detected());

        let s = RobocopyStatus::new(5);
        assert!(s.files_copied() && s.mismatch_detected() && !s.extra_detected());

        let s = RobocopyStatus::new(9);
        assert!(s.files_copied() && s.copy_errors() && !s.fatal());

        let s = RobocopyStatus::new(16);
        assert!(s.fatal() && !s.files_copied() && !s.copy_errors());
    }

    #[test]
    fn retry_only_for_transient_copy_errors() {
        assert!(
            RobocopyStatus::new(8).should_retry(),
            "retry limit exceeded is transient"
        );
        assert!(RobocopyStatus::new(9).should_retry());
        assert!(RobocopyStatus::new(11).should_retry());
        assert!(
            !RobocopyStatus::new(16).should_retry(),
            "fatal error is not transient"
        );
        assert!(
            !RobocopyStatus::new(24).should_retry(),
            "fatal bit wins over copy errors"
        );
        assert!(!RobocopyStatus::new(4).should_retry());
    }

    #[test]
    fn abnormal_termination_is_retried() {
        let s = RobocopyStatus::new(-1);
        assert!(s.is_abnormal());
        assert!(!s.is_success());
        assert!(s.should_retry());
        assert!(s.describe().contains("abnormal"));
    }

    #[test]
    fn describe_lists_every_active_bit() {
        let text = RobocopyStatus::new(0x0F).describe();
        assert!(text.contains("files copied"));
        assert!(text.contains("extra files"));
        assert!(text.contains("mismatched"));
        assert!(text.contains("could not be copied"));
    }

    #[test]
    fn describe_flags_unknown_bits() {
        assert!(RobocopyStatus::new(64).describe().contains("unrecognised"));
        assert!(RobocopyStatus::new(65)
            .describe()
            .contains("unrecognised bits"));
    }
}

//! Bucket utilities — reserved for future bucket-level operations
//! (e.g., filtering to installed processes, bucket-level statistics).

#[cfg(test)]
mod tests {
    use crate::config::{Action, Bucket};

    #[test]
    fn test_bucket_has_processes() {
        let bucket = Bucket {
            name: "Test".into(),
            processes: vec!["chrome.exe".into(), "firefox.exe".into()],
            timeout_mins: 15,
            action: Action::Minimize,
            enabled: true,
            expanded: true,
            icon: None,
        };
        assert_eq!(bucket.processes.len(), 2);
    }
}

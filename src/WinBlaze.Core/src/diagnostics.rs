use std::collections::BTreeMap;

use crate::scan::{ScanEvent, ScanIssueKind, ScanIssueRecord};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScanIssueSummary {
    pub total: u64,
    pub by_kind: BTreeMap<ScanIssueKind, u64>,
    pub recent: Vec<ScanIssueRecord>,
}

impl ScanIssueSummary {
    pub fn from_events(events: impl IntoIterator<Item = ScanEvent>, recent_limit: usize) -> Self {
        let mut summary = Self::default();
        for event in events {
            if let ScanEvent::Issue(issue) = event {
                summary.record(issue, recent_limit);
            }
        }
        summary
    }

    pub fn record(&mut self, issue: ScanIssueRecord, recent_limit: usize) {
        self.total = self.total.saturating_add(1);
        *self.by_kind.entry(issue.kind).or_insert(0) += 1;
        if recent_limit > 0 {
            self.recent.push(issue);
            if self.recent.len() > recent_limit {
                let overflow = self.recent.len() - recent_limit;
                self.recent.drain(0..overflow);
            }
        }
    }

    pub fn count(&self, kind: ScanIssueKind) -> u64 {
        self.by_kind.get(&kind).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(kind: ScanIssueKind, path: &str) -> ScanEvent {
        ScanEvent::Issue(ScanIssueRecord {
            kind,
            path: Some(path.to_string()),
            message: format!("{path} message"),
        })
    }

    #[test]
    fn issue_summary_counts_by_kind_and_bounds_recent_items() {
        let summary = ScanIssueSummary::from_events(
            [
                issue(ScanIssueKind::NotFound, "missing-a"),
                issue(ScanIssueKind::PermissionDenied, "denied"),
                issue(ScanIssueKind::NotFound, "missing-b"),
                ScanEvent::Completed,
            ],
            2,
        );

        assert_eq!(summary.total, 3);
        assert_eq!(summary.count(ScanIssueKind::NotFound), 2);
        assert_eq!(summary.count(ScanIssueKind::PermissionDenied), 1);
        assert_eq!(summary.count(ScanIssueKind::TransientIo), 0);
        assert_eq!(summary.recent.len(), 2);
        assert_eq!(summary.recent[0].path.as_deref(), Some("denied"));
        assert_eq!(summary.recent[1].path.as_deref(), Some("missing-b"));
    }
}

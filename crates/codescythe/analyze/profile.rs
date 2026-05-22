use anyhow::Result;

#[cfg(feature = "profiling")]
mod enabled {
    use super::*;
    use crate::analyze::resolver::ResolverProfileStats;
    use std::{
        env,
        time::{Duration, Instant},
    };

    const PROFILE_ENV: &str = "CODESCYTHE_PROFILE";

    pub(crate) struct AnalysisProfile {
        enabled: bool,
        started: Option<Instant>,
        stages: Vec<AnalysisProfileStage>,
        frontier_batches: usize,
        frontier_files: usize,
        frontier_parse: Duration,
        frontier_inspect: Duration,
    }

    pub(crate) struct AnalysisProfileTimer(Option<Instant>);

    struct AnalysisProfileStage {
        name: &'static str,
        duration: Duration,
    }

    pub(crate) struct AnalysisProfileReport {
        pub(crate) project_files: usize,
        pub(crate) entry_files: usize,
        pub(crate) test_files: usize,
        pub(crate) parsed_files: usize,
        pub(crate) used_files: usize,
        pub(crate) used_exports: usize,
        pub(crate) issue_files: usize,
        pub(crate) issue_exports: usize,
        pub(crate) unresolved: usize,
        pub(crate) ignored_unresolved: usize,
        pub(crate) resolver: Option<ResolverProfileStats>,
    }

    impl AnalysisProfile {
        pub(crate) fn new() -> Self {
            let enabled = profile_enabled();
            Self {
                enabled,
                started: enabled.then(Instant::now),
                stages: Vec::new(),
                frontier_batches: 0,
                frontier_files: 0,
                frontier_parse: Duration::ZERO,
                frontier_inspect: Duration::ZERO,
            }
        }

        pub(crate) fn start(&self) -> AnalysisProfileTimer {
            AnalysisProfileTimer(self.enabled.then(Instant::now))
        }

        pub(crate) fn time<T>(
            &mut self,
            name: &'static str,
            run: impl FnOnce() -> Result<T>,
        ) -> Result<T> {
            let started = self.start();
            let result = run();
            self.record(name, started);
            result
        }

        pub(crate) fn record(&mut self, name: &'static str, started: AnalysisProfileTimer) {
            if let Some(started) = started.0 {
                self.stages.push(AnalysisProfileStage {
                    name,
                    duration: started.elapsed(),
                });
            }
        }

        pub(crate) fn record_frontier(&mut self, files: usize) {
            if self.enabled {
                self.frontier_batches += 1;
                self.frontier_files += files;
            }
        }

        pub(crate) fn record_frontier_parse(&mut self, started: AnalysisProfileTimer) {
            if let Some(started) = started.0 {
                self.frontier_parse += started.elapsed();
            }
        }

        pub(crate) fn record_frontier_inspect(&mut self, started: AnalysisProfileTimer) {
            if let Some(started) = started.0 {
                self.frontier_inspect += started.elapsed();
            }
        }

        pub(crate) fn print(&self, report: AnalysisProfileReport) {
            if !self.enabled {
                return;
            }

            eprintln!("codescythe profile:");
            eprintln!(
                "  total: {}",
                format_duration(self.started.expect("profile is enabled").elapsed())
            );
            eprintln!("  stages:");
            for stage in &self.stages {
                eprintln!("    {:<34} {}", stage.name, format_duration(stage.duration));
            }
            eprintln!(
                "  graph frontiers: batches={}, files={}, parse={}, inspect={}",
                self.frontier_batches,
                self.frontier_files,
                format_duration(self.frontier_parse),
                format_duration(self.frontier_inspect)
            );
            eprintln!(
                "  files: project={}, entries={}, tests={}, parsed={}, used={}",
                report.project_files,
                report.entry_files,
                report.test_files,
                report.parsed_files,
                report.used_files
            );
            eprintln!(
                "  exports/issues: used_exports={}, unused_files={}, unused_exports={}, unresolved={}, ignored_unresolved={}",
                report.used_exports,
                report.issue_files,
                report.issue_exports,
                report.unresolved,
                report.ignored_unresolved
            );
            if let Some(resolver) = report.resolver {
                eprintln!(
                    "  resolver: calls={}, cache_hits={}, cache_misses={}, hit_rate={:.1}%, project={}, external={}, unresolved={}, time={}",
                    resolver.calls,
                    resolver.cache_hits,
                    resolver.cache_misses,
                    resolver.hit_rate(),
                    resolver.project,
                    resolver.external,
                    resolver.unresolved,
                    format_duration(resolver.resolve_time)
                );
            }
        }
    }

    pub(crate) fn profile_enabled() -> bool {
        env::var(PROFILE_ENV)
            .ok()
            .is_some_and(|value| !matches!(value.as_str(), "" | "0" | "false" | "FALSE"))
    }

    fn format_duration(duration: Duration) -> String {
        let millis = duration.as_secs_f64() * 1000.0;
        if millis >= 1000.0 {
            format!("{:.2}s", millis / 1000.0)
        } else {
            format!("{millis:.1}ms")
        }
    }
}

#[cfg(not(feature = "profiling"))]
mod disabled {
    use super::*;

    pub(crate) struct AnalysisProfile;

    pub(crate) struct AnalysisProfileTimer;

    impl AnalysisProfile {
        pub(crate) fn new() -> Self {
            Self
        }

        pub(crate) fn start(&self) -> AnalysisProfileTimer {
            AnalysisProfileTimer
        }

        pub(crate) fn time<T>(
            &mut self,
            _name: &'static str,
            run: impl FnOnce() -> Result<T>,
        ) -> Result<T> {
            run()
        }

        pub(crate) fn record(&mut self, _name: &'static str, _started: AnalysisProfileTimer) {}

        pub(crate) fn record_frontier(&mut self, _files: usize) {}

        pub(crate) fn record_frontier_parse(&mut self, _started: AnalysisProfileTimer) {}

        pub(crate) fn record_frontier_inspect(&mut self, _started: AnalysisProfileTimer) {}
    }
}

#[cfg(not(feature = "profiling"))]
pub(super) use disabled::AnalysisProfile;
#[cfg(feature = "profiling")]
pub(super) use enabled::{AnalysisProfile, AnalysisProfileReport, profile_enabled};

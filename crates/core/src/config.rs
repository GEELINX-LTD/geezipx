//! Compression configuration types.
//!
//! Provides [`CompressOptions`] — a shared options bag that carries
//! compression level and multi-threading configuration through the
//! core/CLI boundary without coupling the two layers.

/// Compression options passed from CLI/UI down to the core engine.
///
/// # Fields
///
/// * `level` — optional compression level.  `None` means "use the
///   format's default level".  The valid range is format-dependent:
///   - gzip/tar.gz/xz/tar.xz: 0..=9
///   - zstd/tar.zst: 0..=22
///   - zip/tar: ignored
///
/// * `jobs` — optional number of worker threads.
///   - `None` / `Some(1)`: single-threaded (default, safe for all formats)
///   - `Some(0)`: automatically use all available logical CPUs
///   - `Some(2..=256)`: explicit thread count
///
///   Only formats whose underlying crates expose a native multi-thread
///   encoder currently act on `jobs` (zstd).  Other formats accept the
///   parameter for forward compatibility but ignore it.
#[derive(Debug, Clone, Copy, Default)]
pub struct CompressOptions {
    pub level: Option<u32>,
    pub jobs: Option<u32>,
}

impl CompressOptions {
    /// Return the effective number of worker threads.
    ///
    /// | `jobs`        | Result                               |
    /// |---------------|--------------------------------------|
    /// | `None`        | 1 (single-threaded, backward compat) |
    /// | `Some(1)`     | 1                                    |
    /// | `Some(0)`     | `available_parallelism()` or 1       |
    /// | `Some(n)`     | `n`                                  |
    pub fn effective_jobs(&self) -> usize {
        match self.jobs {
            None | Some(1) => 1,
            Some(0) => std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1),
            Some(n) => n as usize,
        }
    }

    /// Return the level, or `None` if level is the format default.
    pub fn level(&self) -> Option<u32> {
        self.level
    }

    /// Builder-style: set the compression level.
    pub fn with_level(mut self, level: Option<u32>) -> Self {
        self.level = level;
        self
    }

    /// Builder-style: set the job count.
    pub fn with_jobs(mut self, jobs: Option<u32>) -> Self {
        self.jobs = jobs;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_jobs_default_is_one() {
        let opts = CompressOptions {
            level: None,
            jobs: None,
        };
        assert_eq!(opts.effective_jobs(), 1);
    }

    #[test]
    fn effective_jobs_explicit_one_is_one() {
        let opts = CompressOptions {
            level: None,
            jobs: Some(1),
        };
        assert_eq!(opts.effective_jobs(), 1);
    }

    #[test]
    fn effective_jobs_explicit_two() {
        let opts = CompressOptions {
            level: None,
            jobs: Some(2),
        };
        assert_eq!(opts.effective_jobs(), 2);
    }

    #[test]
    fn effective_jobs_explicit_four() {
        let opts = CompressOptions {
            level: None,
            jobs: Some(4),
        };
        assert_eq!(opts.effective_jobs(), 4);
    }

    #[test]
    fn effective_jobs_zero_auto_at_least_one() {
        let opts = CompressOptions {
            level: None,
            jobs: Some(0),
        };
        assert!(opts.effective_jobs() >= 1, "jobs=0 should yield >=1");
    }
}

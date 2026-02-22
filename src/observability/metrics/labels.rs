/// scratch buffer types for chromatic aberration effect.
///
/// tracks memory allocation patterns across ca operations.
/// used by `record_scratch_capacity` to monitor buffer usage.
#[derive(Clone, Copy)]
pub enum ScratchBuffer {
    Downscaled = 0,
    Working = 1,
    FullResult = 2,
}

impl ScratchBuffer {
    pub(super) const COUNT: usize = 3;
    pub(super) const ALL: [Self; Self::COUNT] = [Self::Downscaled, Self::Working, Self::FullResult];

    #[inline]
    pub(super) const fn as_index(self) -> usize {
        self as usize
    }

    #[inline]
    pub(super) const fn as_name(self) -> &'static str {
        match self {
            Self::Downscaled => "ca_downscaled",
            Self::Working => "ca_working",
            Self::FullResult => "ca_full_result",
        }
    }
}

/// cache layer labels for cache operation metrics.
///
/// distinguishes between:
/// - `source`: raw object bytes from storage
/// - `result`: transformed image output
/// - `version`: object version tokens for cache invalidation
/// - `hot_result`: in-process lru cache for hot entries
#[derive(Clone, Copy)]
pub enum ExternalCacheLayer {
    Source = 0,
    Result = 1,
    Version = 2,
    HotResult = 3,
}

impl ExternalCacheLayer {
    pub(super) const COUNT: usize = 4;
    pub(super) const ALL: [Self; Self::COUNT] =
        [Self::Source, Self::Result, Self::Version, Self::HotResult];

    #[inline]
    pub(super) const fn as_index(self) -> usize {
        self as usize
    }

    #[inline]
    pub const fn as_name(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Result => "result",
            Self::Version => "version",
            Self::HotResult => "hot_result",
        }
    }
}

/// pipeline stage labels for latency tracking.
///
/// tracks time spent in each major phase:
/// - `object_fetch`: object storage retrieval with retries
/// - `decode`: image format decoding (runs in blocking thread)
/// - `transform`: geometric and pixel-level image processing
/// - `encode`: webp encoding with quality adjustment
#[derive(Clone, Copy)]
pub enum PipelineStage {
    ObjectFetch = 0,
    Decode = 1,
    Transform = 2,
    Encode = 3,
}

impl PipelineStage {
    pub(super) const COUNT: usize = 4;
    pub(super) const ALL: [Self; Self::COUNT] = [
        Self::ObjectFetch,
        Self::Decode,
        Self::Transform,
        Self::Encode,
    ];

    #[inline]
    pub(super) const fn as_index(self) -> usize {
        self as usize
    }

    #[inline]
    pub(super) const fn as_name(self) -> &'static str {
        match self {
            Self::ObjectFetch => "object_fetch",
            Self::Decode => "decode",
            Self::Transform => "transform",
            Self::Encode => "encode",
        }
    }
}

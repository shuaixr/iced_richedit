//! Opt-in CPU-stage elapsed timing and event counters.

use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    time::{Duration, Instant},
};

use serde_json::{Map, Value, json};

const RECENT_SAMPLE_CAPACITY: usize = 128;
const SLOW_SAMPLE_CAPACITY: usize = 8;
const HISTOGRAM_UPPER_BOUNDS_US: [u64; 10] = [
    100, 250, 500, 1_000, 2_000, 4_000, 8_000, 16_000, 32_000, 64_000,
];
const HISTOGRAM_BUCKET_COUNT: usize = HISTOGRAM_UPPER_BOUNDS_US.len() + 1;
const REPORT_SCHEMA_VERSION: u32 = 2;
const INSTRUMENTATION_REVISION: u32 = 1;

/// A bounded set of CPU timing categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Timing {
    Edit,
    Projection,
    CompositionProjection,
    Layout,
    HitTest,
    VisualNavigation,
    Draw,
    KeyboardEvent,
    PointerEvent,
    ImeEvent,
    RedrawEvent,
    OtherEvent,
}

impl Timing {
    fn name(self) -> &'static str {
        match self {
            Self::Edit => "edit",
            Self::Projection => "projection",
            Self::CompositionProjection => "composition_projection",
            Self::Layout => "layout",
            Self::HitTest => "hit_test",
            Self::VisualNavigation => "visual_navigation",
            Self::Draw => "draw",
            Self::KeyboardEvent => "keyboard_event",
            Self::PointerEvent => "pointer_event",
            Self::ImeEvent => "ime_event",
            Self::RedrawEvent => "redraw_event",
            Self::OtherEvent => "other_event",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Edit => {
                "Host instrumented editor command execution, including selection changes."
            }
            Self::Projection => "Document to view projection without an active composition.",
            Self::CompositionProjection => {
                "Document to view projection with an active composition."
            }
            Self::Layout => {
                "RichEdit layout processing, including projection and paragraph cache work."
            }
            Self::HitTest => "Instrumented pointer to document position queries.",
            Self::VisualNavigation => {
                "Visual caret target queries for vertical and line boundary navigation."
            }
            Self::Draw => {
                "RichEdit CPU draw submission, excluding full rendering and presentation."
            }
            Self::KeyboardEvent => {
                "Synchronous RichEdit keyboard event handling, excluding later host updates."
            }
            Self::PointerEvent => "Synchronous RichEdit pointer event handling.",
            Self::ImeEvent => "Synchronous RichEdit input method event handling.",
            Self::RedrawEvent => "Synchronous RichEdit redraw event handling.",
            Self::OtherEvent => {
                "Synchronous RichEdit handling of other instrumented widget events."
            }
        }
    }
}

/// A bounded set of operations attributed to aggregate timing categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Operation {
    EditInput,
    EditDelete,
    EditSelection,
    EditHistory,
    EditFormat,
    EditPaste,
    EditImeCommit,
    NavigationVertical,
    NavigationLineBoundary,
    HitTestClick,
    HitTestDrag,
}

impl Operation {
    fn timing(self) -> Timing {
        match self {
            Self::EditInput
            | Self::EditDelete
            | Self::EditSelection
            | Self::EditHistory
            | Self::EditFormat
            | Self::EditPaste
            | Self::EditImeCommit => Timing::Edit,

            Self::NavigationVertical | Self::NavigationLineBoundary => Timing::VisualNavigation,

            Self::HitTestClick | Self::HitTestDrag => Timing::HitTest,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::EditInput => "edit_input",
            Self::EditDelete => "edit_delete",
            Self::EditSelection => "edit_selection",
            Self::EditHistory => "edit_history",
            Self::EditFormat => "edit_format",
            Self::EditPaste => "edit_paste",
            Self::EditImeCommit => "edit_ime_commit",
            Self::NavigationVertical => "navigation_vertical",
            Self::NavigationLineBoundary => "navigation_line_boundary",
            Self::HitTestClick => "hit_test_click",
            Self::HitTestDrag => "hit_test_drag",
        }
    }
}

/// Paragraph cache decisions counted during a measurement window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Counter {
    ParagraphReused,
    ParagraphResized,
    ParagraphRebuilt,
}

impl Counter {
    fn name(self) -> &'static str {
        match self {
            Self::ParagraphReused => "paragraph_reused",
            Self::ParagraphResized => "paragraph_resized",
            Self::ParagraphRebuilt => "paragraph_rebuilt",
        }
    }
}

/// Records one operation when it leaves its lexical scope.
#[derive(Debug)]
#[must_use = "keep the guard alive for the operation being measured"]
pub struct TimingScope<'a> {
    diagnostics: &'a Diagnostics,
    generation: u64,
    timing: Timing,
    started_at: Instant,
    operation: Option<Operation>,
}

impl Drop for TimingScope<'_> {
    fn drop(&mut self) {
        let elapsed = self.started_at.elapsed();
        let mut session = self.diagnostics.session.borrow_mut();

        if session.generation != self.generation {
            return;
        }

        let Some(window_started_at) = session.started_at else {
            return;
        };

        let start_offset = self.started_at.duration_since(window_started_at);

        session
            .timings
            .entry(self.timing)
            .or_default()
            .record(elapsed, Some(start_offset));

        if let Some(operation) = self.operation {
            session
                .operations
                .entry(operation)
                .or_default()
                .record(elapsed, Some(start_offset));
        }
    }
}
/// A single-threaded recorder owned separately from editor document state.
///
/// Shared references can record events without holding a mutable borrow
/// across the operation being measured. This is not a global recorder.
#[derive(Debug, Default)]
pub struct Diagnostics {
    session: RefCell<MeasurementSession>,
}

impl Diagnostics {
    /// Starts a new measurement window, discarding previous measurements.
    pub fn start(&self) {
        let mut session = self.session.borrow_mut();

        let generation = session
            .generation
            .checked_add(1)
            .expect("measurement generation exhausted");

        *session = MeasurementSession {
            generation,
            ..MeasurementSession::default()
        };

        session.started_at = Some(Instant::now());
    }

    /// Starts timing an operation in the current measurement window.
    ///
    /// Keep the returned guard alive until the operation finishes.
    /// No clock is read when recording is inactive.
    ///
    /// The guard is ignored if recording stops or a new window starts
    /// before the guard is dropped.
    pub fn scope(&self, timing: Timing) -> Option<TimingScope<'_>> {
        let generation = {
            let session = self.session.borrow();

            session.started_at?;
            session.generation
        };

        Some(TimingScope {
            diagnostics: self,
            generation,
            timing,
            started_at: Instant::now(),
            operation: None,
        })
    }

    /// Stops recording while retaining the measurements.
    ///
    /// Calling this more than once leaves the stopped report unchanged.
    pub fn stop(&self) {
        let mut session = self.session.borrow_mut();

        if let Some(started_at) = session.started_at.take() {
            session.elapsed = started_at.elapsed();
        }
    }

    pub fn is_recording(&self) -> bool {
        self.session.borrow().started_at.is_some()
    }

    /// Records one completed CPU-stage operation.
    ///
    /// Events outside an active measurement window are ignored.
    /// The operation's start offset is unknown because only its duration
    /// is supplied.
    pub fn record_duration(&self, timing: Timing, duration: Duration) {
        let mut session = self.session.borrow_mut();

        if session.started_at.is_none() {
            return;
        }

        session
            .timings
            .entry(timing)
            .or_default()
            .record(duration, None);
    }

    /// Records one cache decision during an active measurement window.
    pub fn increment(&self, counter: Counter) {
        let mut session = self.session.borrow_mut();

        if session.started_at.is_none() {
            return;
        }

        *session.counters.entry(counter).or_default() += 1;
    }

    /// Returns a JSON report without changing the recording state.
    ///
    /// Unobserved timings and counters are omitted. Percentiles describe
    /// the most recent samples, not necessarily the entire measurement.
    /// Report generation sorts copies of those bounded samples.
    pub fn snapshot(&self) -> Value {
        let session = self.session.borrow();

        let elapsed = session
            .started_at
            .map_or(session.elapsed, |started_at| started_at.elapsed());

        let timings: Map<String, Value> = session
            .timings
            .iter()
            .map(|(timing, stats)| (timing.name().to_owned(), stats.snapshot()))
            .collect();

        let counters: Map<String, Value> = session
            .counters
            .iter()
            .map(|(counter, count)| (counter.name().to_owned(), json!(count)))
            .collect();

        let operations: Map<String, Value> = session
            .operations
            .iter()
            .map(|(operation, stats)| (operation.name().to_owned(), stats.snapshot()))
            .collect();
        let timing_definitions: Map<String, Value> = session
            .timings
            .keys()
            .map(|timing| {
                (
                    timing.name().to_owned(),
                    json!({
                        "description": timing.description()
                    }),
                )
            })
            .collect();

        let operation_definitions: Map<String, Value> = session
            .operations
            .keys()
            .map(|operation| {
                (
                    operation.name().to_owned(),
                    json!({
                        "aggregate_timing": operation.timing().name(),
                        "relationship": "same_sample_recorded_in_both_categories"
                    }),
                )
            })
            .collect();
        json!({
            "schema_version": REPORT_SCHEMA_VERSION,
            "instrumentation_revision": INSTRUMENTATION_REVISION,
            "environment": {
                "package_version": env!("CARGO_PKG_VERSION"),
                "os": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
                "debug_assertions": cfg!(debug_assertions)
            },
            "recording": session.started_at.is_some(),
            "elapsed_ms": milliseconds(elapsed),
            "measurement_contract": {
                "clock": "std::time::Instant",
                "duration_kind": "monotonic_elapsed_time",
                "duration_unit": "milliseconds",
                "includes_thread_descheduling": true,
                "window_elapsed_includes_idle": true,
                "recommended_export_state": "stopped",
                "metric_definitions_scope": "observed_categories_only",
                "omitted_category": "not_observed_not_zero",
                "unknown_value": "null",
                "total_semantics": "sum_of_recorded_sample_durations",
                "maximum_semantics": "maximum_recorded_sample_in_entire_window",
                "overlap": {
                    "totals_are_additive": false,
                    "percentiles_are_additive": false,
                    "operation_and_aggregate": "same_sample_not_additional_work",
                    "may_contain": [
                        {
                            "outer": Timing::Layout.name(),
                            "inner": [
                                Timing::Projection.name(),
                                Timing::CompositionProjection.name()
                            ]
                        },
                        {
                            "outer": Timing::KeyboardEvent.name(),
                            "inner": [Timing::VisualNavigation.name()]
                        },
                        {
                            "outer": Timing::PointerEvent.name(),
                            "inner": [Timing::HitTest.name()]
                        }
                    ],
                    "host_edit_dispatch": "separate_from_synchronous_widget_event_handling"
                },
                "recorder_overhead": {
                    "zero_overhead_claimed": false,
                    "own_scope_statistics_update": "excluded_from_own_duration",
                    "nested_statistics_update": "may_be_included_in_enclosing_scopes",
                    "overhead_percentage": null
                },
                "not_measured": [
                    "thread_cpu_time",
                    "gpu_time",
                    "complete_frame_time",
                    "presentation_latency",
                    "input_to_display_latency",
                    "actual_frames_per_second"
                ],
                "privacy": {
                    "document_content_recorded": false,
                    "input_text_recorded": false,
                    "clipboard_content_recorded": false,
                    "event_objects_recorded": false,
                    "automatic_git_lookup": false
                },
                "comparison": {
                    "matching_versions_required": true,
                    "matching_versions_sufficient": false,
                    "also_check": [
                        "code_version",
                        "document_scale_and_state",
                        "viewport",
                        "operation_mix",
                        "warmup",
                        "build_configuration",
                        "platform",
                        "renderer_backend"
                    ],
                    "debug_assertions_identifies_optimization_profile": false
                }
            },
            "sampling": {
                "recent": {
                    "scope": "most_recent_per_timing_or_operation",
                    "capacity": RECENT_SAMPLE_CAPACITY,
                    "percentile_method": "nearest_rank",
                    "percentiles_are_exact_for_retained_samples": true
                },
                "histogram": {
                    "scope": "entire_measurement_window",
                    "upper_bounds_ms": histogram_upper_bounds_ms(),
                    "count_semantics": "non_cumulative",
                    "first_lower_bound_ms": 0.0,
                    "first_lower_inclusive": true,
                    "subsequent_lower_inclusive": false,
                    "finite_upper_inclusive": true,
                    "null_upper_bound": "positive_infinity"
                },
                "slowest": {
                    "scope": "entire_measurement_window",
                    "capacity": SLOW_SAMPLE_CAPACITY,
                    "order": "duration_desc_then_sequence_asc",
                    "sequence_scope": "per_category_record_order",
                    "start_offset_origin": "measurement_window_start",
                    "start_offset_position": "operation_start",
                    "null_start_offset": "unknown",
                    "cross_category_event_id": false
                }
            },
            "metric_definitions": {
                "timings": timing_definitions,
                "operations": operation_definitions
            },
            "timings": timings,
            "operations": operations,
            "counters": counters,
            "layout_context": {
                "viewport": session.viewport.as_ref().map(ViewportStats::snapshot),
                "projected_blocks": session.projected_blocks.as_ref().map(CountRange::snapshot),
                "invalid_viewport_observations": session.invalid_viewport_observations
            }
        })
    }

    /// Records the same duration in both the operation and its aggregate timing.
    pub fn operation_scope(&self, operation: Operation) -> Option<TimingScope<'_>> {
        let mut scope = self.scope(operation.timing())?;
        scope.operation = Some(operation);
        Some(scope)
    }

    /// Observes resolved widget geometry and an optional successful projection.
    ///
    /// Zero dimensions are valid observations. Invalid dimensions are counted
    /// separately. No observations are changed outside an active window.
    pub fn observe_layout(&self, width: f32, height: f32, projected_blocks: Option<usize>) {
        let mut session = self.session.borrow_mut();

        if session.started_at.is_none() {
            return;
        }

        if width.is_finite() && height.is_finite() && width >= 0.0 && height >= 0.0 {
            let size = [width, height];

            match &mut session.viewport {
                Some(viewport) => viewport.observe(size),
                None => session.viewport = Some(ViewportStats::new(size)),
            }
        } else {
            session.invalid_viewport_observations += 1;
        }

        if let Some(count) = projected_blocks {
            match &mut session.projected_blocks {
                Some(range) => range.observe(count),
                None => session.projected_blocks = Some(CountRange::new(count)),
            }
        }
    }
}

#[derive(Debug, Default)]
struct MeasurementSession {
    generation: u64,
    started_at: Option<Instant>,
    elapsed: Duration,
    timings: BTreeMap<Timing, TimingStats>,
    operations: BTreeMap<Operation, TimingStats>,
    counters: BTreeMap<Counter, u64>,
    viewport: Option<ViewportStats>,
    projected_blocks: Option<CountRange>,
    invalid_viewport_observations: u64,
}

#[derive(Debug)]
struct ViewportStats {
    first: [f32; 2],
    last: [f32; 2],
    minimum: [f32; 2],
    maximum: [f32; 2],
    observations: u64,
    changes: u64,
}

impl ViewportStats {
    fn new(size: [f32; 2]) -> Self {
        Self {
            first: size,
            last: size,
            minimum: size,
            maximum: size,
            observations: 1,
            changes: 0,
        }
    }

    fn observe(&mut self, size: [f32; 2]) {
        if self.last != size {
            self.changes += 1;
        }

        self.last = size;

        for (index, value) in size.into_iter().enumerate() {
            self.minimum[index] = self.minimum[index].min(value);
            self.maximum[index] = self.maximum[index].max(value);
        }

        self.observations += 1;
    }

    fn snapshot(&self) -> Value {
        json!({
            "source": "resolved_widget_layout_size",
            "units": "logical_pixels",
            "first": dimensions(self.first),
            "last": dimensions(self.last),
            "minimum": dimensions(self.minimum),
            "maximum": dimensions(self.maximum),
            "observations": self.observations,
            "changes": self.changes
        })
    }
}

#[derive(Debug)]
struct CountRange {
    first: usize,
    last: usize,
    minimum: usize,
    maximum: usize,
    observations: u64,
}

impl CountRange {
    fn new(value: usize) -> Self {
        Self {
            first: value,
            last: value,
            minimum: value,
            maximum: value,
            observations: 1,
        }
    }

    fn observe(&mut self, value: usize) {
        self.last = value;
        self.minimum = self.minimum.min(value);
        self.maximum = self.maximum.max(value);
        self.observations += 1;
    }

    fn snapshot(&self) -> Value {
        json!({
            "first": self.first,
            "last": self.last,
            "minimum": self.minimum,
            "maximum": self.maximum,
            "observations": self.observations
        })
    }
}

fn dimensions([width, height]: [f32; 2]) -> Value {
    json!({
        "width": width,
        "height": height
    })
}

#[derive(Debug)]
struct SlowSample {
    sequence: u64,
    duration: Duration,
    start_offset: Option<Duration>,
}

impl SlowSample {
    fn snapshot(&self) -> Value {
        json!({
            "sequence": self.sequence,
            "duration_ms": milliseconds(self.duration),
            "start_offset_ms": self.start_offset.map(milliseconds)
        })
    }
}

#[derive(Debug, Default)]
struct TimingStats {
    count: u64,
    total_ms: f64,
    maximum: Duration,
    recent: VecDeque<Duration>,
    histogram: [u64; HISTOGRAM_BUCKET_COUNT],
    slowest: Vec<SlowSample>,
}

impl TimingStats {
    fn record(&mut self, duration: Duration, start_offset: Option<Duration>) {
        self.count += 1;
        self.total_ms += milliseconds(duration);
        self.maximum = self.maximum.max(duration);

        if self.recent.len() == RECENT_SAMPLE_CAPACITY {
            self.recent.pop_front();
        }

        self.recent.push_back(duration);

        let bucket = HISTOGRAM_UPPER_BOUNDS_US
            .iter()
            .position(|&upper| duration <= Duration::from_micros(upper))
            .unwrap_or(HISTOGRAM_UPPER_BOUNDS_US.len());

        self.histogram[bucket] += 1;

        let insertion_index = self
            .slowest
            .iter()
            .position(|sample| sample.duration < duration)
            .unwrap_or(self.slowest.len());

        if insertion_index < SLOW_SAMPLE_CAPACITY {
            if self.slowest.len() == SLOW_SAMPLE_CAPACITY {
                self.slowest.pop();
            }

            self.slowest.insert(
                insertion_index,
                SlowSample {
                    sequence: self.count,
                    duration,
                    start_offset,
                },
            );
        }
    }

    fn snapshot(&self) -> Value {
        let mut ordered: Vec<_> = self.recent.iter().copied().collect();
        ordered.sort_unstable();

        let slowest: Vec<_> = self.slowest.iter().map(SlowSample::snapshot).collect();

        json!({
            "count": self.count,
            "total_ms": self.total_ms,
            "max_ms": milliseconds(self.maximum),
            "recent": {
                "count": ordered.len(),
                "p50_ms": percentile(&ordered, 50),
                "p95_ms": percentile(&ordered, 95),
                "max_ms": ordered.last().copied().map(milliseconds)
            },
            "histogram_counts": self.histogram,
            "slowest": slowest
        })
    }
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn histogram_upper_bounds_ms() -> Vec<Option<f64>> {
    HISTOGRAM_UPPER_BOUNDS_US
        .iter()
        .map(|&upper| Some(milliseconds(Duration::from_micros(upper))))
        .chain(std::iter::once(None))
        .collect()
}

fn percentile(ordered: &[Duration], percent: usize) -> Option<f64> {
    if ordered.is_empty() {
        return None;
    }

    let rank = (ordered.len() * percent).div_ceil(100);

    Some(milliseconds(ordered[rank - 1]))
}

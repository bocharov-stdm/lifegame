//! Many runs at once: every variant of a plan on every seed, each as its own `life-report`
//! process, then a summary per variant. For balance searches ("which formula lets carnivores and
//! herbivores coexist?") without babysitting dozens of terminals.
//!
//!     cargo build -p life-report --release
//!     target/release/life-sweep plan.txt --out sweeps/hunt --jobs 16 --seconds 300
//!
//! The plan is a text file:
//!
//!     # the baseline conditions, shared by every variant
//!     seeds: 1 2 3 4 5 6 7 8
//!     args: --scale 20 --shape 2:1 --rule cost_scale=3 --ticks 20000
//!     variant control:
//!     variant smell2: --rule carnivore_smell=2
//!     variant young: EXP_YOUNG_PLANTS=0.7 --rule carnivore_smell=1.5
//!
//! A `NAME=VALUE` token in capitals is an environment variable of that variant's processes (for
//! experiment builds that read switches from the environment); everything else is passed to
//! `life-report` after the shared `args`. Comments describe: the block at the top describes the
//! plan, a block right above variants describes them (until `seeds:`/`args:` or another block),
//! and a comment at the end of a variant's line describes that one variant.
//!
//! Progress goes to `OUT/progress.json` (rewritten every second) and, unless `--no-window`, a
//! small always-on-top window shows it: `life-progress` (the `life-app` crate) next to this
//! program, or `--viewer PATH`. It shows the bar, the time left, what is running now with its
//! description, and the same progress on the taskbar button.
//!
//! Guards, so a sweep always ends: every run gets the report's own deadline (`--seconds`; a run
//! whose ticks slow down stops there and is counted as cut, never as finished), and a watchdog
//! kills a process that outlives the deadline by `--grace` seconds. The worst case is printed
//! before the start, the expected end after every finished run. `--max-work` defaults to 1e15 so
//! runs end on their own, not on the work budget.
//!
//! Results: `OUT/<variant>/s<seed>.json` and `.txt` (the report's JSON and text), `OUT/runs.csv`
//! (one row per run), `OUT/summary.csv` and `OUT/summary.md` (one row per variant). A run whose
//! JSON and command line are already in `OUT` is reused, so a stopped sweep resumes where it was.
//! Lines `METRIC <name> <number>` in a run's text output become extra columns (for experiment
//! builds).

use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use clap::Parser;
use life_core::rules::DIETS;
use serde_json::{Value, json};

#[derive(Parser, Debug)]
#[command(about = "Run a plan of variants x seeds through life-report and summarise each variant")]
struct Args {
    /// The plan file (see the module docs).
    plan: PathBuf,
    /// Where the runs and the summary go.
    #[arg(long)]
    out: PathBuf,
    /// Runs at a time, each on one thread (default: logical CPUs minus 4).
    #[arg(long)]
    jobs: Option<usize>,
    /// Deadline of one run, seconds (the report's `--seconds`).
    #[arg(long, default_value_t = 300)]
    seconds: u64,
    /// A process still alive this long after its deadline is killed.
    #[arg(long, default_value_t = 60)]
    grace: u64,
    /// The report executable (default: `life-report` next to this program).
    #[arg(long)]
    exe: Option<PathBuf>,
    /// Share of the run at its end over which "late" means are taken.
    #[arg(long, default_value_t = 0.25)]
    late: f64,
    /// Summarise what is in `--out` without running anything.
    #[arg(long)]
    summary_only: bool,
    /// Rerun even runs already in `--out`.
    #[arg(long)]
    fresh: bool,
    /// Do not open the progress window.
    #[arg(long)]
    no_window: bool,
    /// The progress window executable (default: `life-progress` next to this program).
    #[arg(long)]
    viewer: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq)]
struct Variant {
    name: String,
    /// What the variant tries, from the plan's comments.
    about: String,
    env: Vec<(String, String)>,
    args: Vec<String>,
}

#[derive(Debug, PartialEq)]
struct Plan {
    /// What the whole plan is about: its first comment block.
    about: String,
    seeds: Vec<u64>,
    args: Vec<String>,
    variants: Vec<Variant>,
}

fn is_env(token: &str) -> bool {
    token.split_once('=').is_some_and(|(k, _)| {
        !k.is_empty()
            && k.chars().next().is_some_and(|c| c.is_ascii_uppercase())
            && k.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
    })
}

/// Flags the sweep sets itself: a plan that sets them too would make the report fail on a
/// repeated flag, or slip past the guards.
const OWN_FLAGS: [&str; 7] =
    ["--seed", "--seeds", "--seconds", "--threads", "--json", "--compare", "--progress"];

fn parse_plan(text: &str) -> Result<Plan, String> {
    let mut plan = Plan { about: String::new(), seeds: Vec::new(), args: Vec::new(), variants: Vec::new() };
    // the comment block being read, and the one describing the variants below it
    let (mut block, mut about, mut in_block, mut seen_any) =
        (Vec::<String>::new(), String::new(), false, false);
    for (n, raw) in text.lines().enumerate() {
        let (code, comment) = raw.split_once('#').map_or((raw, None), |(c, k)| (c, Some(k.trim())));
        let line = code.trim();
        if line.is_empty() {
            if let Some(k) = comment {
                if !in_block {
                    block.clear();
                }
                in_block = true;
                block.push(k.to_string());
            }
            continue;
        }
        if in_block {
            about = block.join(" ");
            if !seen_any && plan.about.is_empty() {
                plan.about = about.clone();
            }
            in_block = false;
        }
        seen_any = true;
        let at = |e: String| format!("plan line {}: {e}", n + 1);
        let tokens = |s: &str| -> Result<Vec<String>, String> {
            let t: Vec<String> = s.split_whitespace().map(str::to_string).collect();
            match t.iter().find(|t| OWN_FLAGS.contains(&flag_name(t))) {
                Some(f) => Err(at(format!("{f} is set by the sweep itself"))),
                None => Ok(t),
            }
        };
        if let Some(rest) = line.strip_prefix("seeds:") {
            about.clear();
            plan.seeds = rest
                .split_whitespace()
                .map(|s| s.parse().map_err(|_| at(format!("«{s}» is not a seed"))))
                .collect::<Result<_, _>>()?;
        } else if let Some(rest) = line.strip_prefix("args:") {
            about.clear();
            plan.args = tokens(rest)?;
        } else if let Some(rest) = line.strip_prefix("variant ") {
            let (name, rest) = rest.split_once(':').ok_or_else(|| at("variant NAME: ARGS".into()))?;
            let name = name.trim().to_string();
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
                return Err(at(format!("variant name «{name}»: letters, digits, _ and - only")));
            }
            if plan.variants.iter().any(|v| v.name == name) {
                return Err(at(format!("variant «{name}» twice")));
            }
            let (env, args): (Vec<String>, Vec<String>) = tokens(rest)?.into_iter().partition(|t| is_env(t));
            let env = env
                .iter()
                .map(|t| t.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())).expect("is_env"))
                .collect();
            let about = comment.filter(|k| !k.is_empty()).map_or_else(|| about.clone(), str::to_string);
            plan.variants.push(Variant { name, about, env, args });
        } else {
            return Err(at(format!("expected seeds:, args: or variant NAME:, got «{line}»")));
        }
    }
    if plan.seeds.is_empty() {
        return Err("the plan has no seeds: line".into());
    }
    if plan.variants.is_empty() {
        return Err("the plan has no variants".into());
    }
    Ok(plan)
}

/// A token's flag without its value: `--max-work=1e14` is `--max-work` too.
fn flag_name(token: &str) -> &str {
    match token.split_once('=') {
        Some((flag, _)) if flag.starts_with("--") => flag,
        _ => token,
    }
}

struct Job {
    variant: Variant,
    seed: u64,
}

fn job_paths(out: &Path, job: &Job) -> (PathBuf, PathBuf, PathBuf) {
    let dir = out.join(&job.variant.name);
    let s = job.seed;
    (dir.join(format!("s{s}.json")), dir.join(format!("s{s}.txt")), dir.join(format!("s{s}.cmd")))
}

fn command_line(exe: &Path, plan: &Plan, job: &Job, seconds: u64, json: &Path) -> Vec<String> {
    let mut line: Vec<String> = job.variant.env.iter().map(|(k, v)| format!("{k}={v}")).collect();
    line.push(exe.display().to_string());
    line.extend(["--seed".into(), job.seed.to_string()]);
    line.extend(plan.args.iter().cloned());
    line.extend(job.variant.args.iter().cloned());
    if !line.iter().any(|t| flag_name(t) == "--max-work") {
        line.extend(["--max-work".into(), "1e15".into()]);
    }
    line.extend(["--seconds".into(), seconds.to_string(), "--threads".into(), "1".into()]);
    line.extend(["--json".into(), json.display().to_string()]);
    line.extend(["--progress".into(), json.with_extension("tick").display().to_string()]);
    line
}

/// A running run's «tick of ticks» and its latest ms a tick (0 before the first lap or from an
/// older report), from the file the report rewrites about once a second.
fn tick_of(out: &Path, variant: &str, seed: u64) -> Option<(u64, u64, f64)> {
    let text = fs::read_to_string(out.join(variant).join(format!("s{seed}.tick"))).ok()?;
    let mut it = text.split_whitespace();
    let tick = it.next()?.parse().ok()?;
    let total = it.next()?.parse().ok().filter(|&t: &u64| t > 0)?;
    Some((tick, total, it.next().and_then(|m| m.parse().ok()).unwrap_or(0.0)))
}

/// A run whose worst lap was this many times slower than its median lap is flagged: in the
/// summary, the progress window and the output.
const SLOWDOWN_WARN: f64 = 3.0;

/// A run's tick rate from the report's `pace` laps (every 500 ticks).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Pace {
    /// The median lap's ms a tick.
    median: f64,
    /// The worst lap over the median, and the tick it ended at.
    slowdown: f64,
    worst_at: u64,
}

fn pace_of(run: &Value) -> Option<Pace> {
    let laps: Vec<(u64, f64)> =
        run["pace"].as_array()?.iter().filter_map(|l| Some((l[0].as_u64()?, l[1].as_f64()?))).collect();
    let median = median(laps.iter().map(|l| l.1).collect());
    let (worst_at, worst) = laps.iter().copied().max_by(|a, b| a.1.total_cmp(&b.1))?;
    (median > 0.0).then_some(Pace { median, slowdown: worst / median, worst_at })
}

/// How a run ended, from the sweep's side.
#[derive(Clone, Debug)]
enum Outcome {
    Reused,
    Exited(Duration),
    Killed(Duration),
    Failed(String),
    /// Taken off by a pause or a stop from the window; after a pause it runs again from the start
    /// (a world depends on its seed only, so the result is the same).
    Interrupted,
}

/// What the window asks for, in `OUT/control.txt` (a variant name has no dot, so no variant folder takes its name): `run`, `pause` or `stop`.
const RUN: u8 = 0;
const PAUSE: u8 = 1;
const STOP: u8 = 2;

fn read_control(out: &Path) -> u8 {
    match fs::read_to_string(out.join("control.txt")).unwrap_or_default().trim() {
        "pause" => PAUSE,
        "stop" => STOP,
        _ => RUN,
    }
}

/// Which build of the report ran: its size and modification time. A rebuilt report (a model
/// change, another experiment build) runs everything again instead of reusing the old model's
/// numbers under the same command line.
fn build_stamp(exe: &Path) -> String {
    let Ok(meta) = fs::metadata(exe) else { return "unknown build".into() };
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    format!("build {} bytes, modified {modified}", meta.len())
}

fn run_job(exe: &Path, plan: &Plan, job: &Job, args: &Args, control: &AtomicU8) -> Outcome {
    let (json, txt, cmd) = job_paths(&args.out, job);
    let line = command_line(exe, plan, job, args.seconds, &json);
    // the key a result is reused by: the command line and the build that ran it
    let joined = format!("{}\n{}", line.join(" "), build_stamp(exe));
    if !args.fresh
        && fs::read_to_string(&cmd).is_ok_and(|c| c == joined)
        && read_json(&json).is_some_and(|v| v["runs"][0]["stop"].is_string())
    {
        return Outcome::Reused;
    }
    let _ = fs::remove_file(&json);
    if let Err(e) = fs::create_dir_all(json.parent().expect("a variant dir")) {
        return Outcome::Failed(e.to_string());
    }
    let Ok(stdout) = fs::File::create(&txt) else { return Outcome::Failed(format!("{}", txt.display())) };
    let Ok(stderr) = stdout.try_clone() else { return Outcome::Failed("stderr".into()) };
    let started = Instant::now();
    let child = Command::new(exe)
        .args(&line[job.variant.env.len() + 1..])
        .envs(job.variant.env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => return Outcome::Failed(format!("{}: {e}", exe.display())),
    };
    let limit = Duration::from_secs(args.seconds + args.grace);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let _ = fs::write(&cmd, &joined);
                return if status.success() {
                    Outcome::Exited(started.elapsed())
                } else {
                    Outcome::Failed(format!("exit {status}"))
                };
            }
            Ok(None) if started.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                return Outcome::Killed(started.elapsed());
            }
            Ok(None) if control.load(Ordering::Relaxed) != RUN => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = fs::remove_file(&json);
                return Outcome::Interrupted;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
            Err(e) => return Outcome::Failed(e.to_string()),
        }
    }
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

/// What one run tells about the balance.
#[derive(Clone, Debug, Default)]
struct RunMetrics {
    seed: u64,
    stop: String,
    ticks: u64,
    ms_per_tick: f64,
    pop_end: f64,
    pop_late: f64,
    pop_min: f64,
    diet_late: [f64; 4],
    diet_end: [f64; 4],
    born: [f64; 4],
    kills_by_carnivores: f64,
    /// The tick rate by laps, when the report measured it.
    pace: Option<Pace>,
    extra: BTreeMap<String, f64>,
}

impl RunMetrics {
    /// Ended on its own: all its ticks, or its world died out.
    fn complete(&self) -> bool {
        self.stop == "done" || self.stop == "extinct"
    }
    /// A diet holds when it keeps at least 10 creatures and 1% of the world over the late window.
    fn holds(&self, diet: usize) -> bool {
        self.diet_late[diet] >= 10.0_f64.max(0.01 * self.pop_late)
    }
}

fn metrics_of(json: &Value, text: &str, seed: u64, late: f64) -> Option<RunMetrics> {
    let run = &json["runs"][0];
    let snaps = run["snapshots"].as_array()?;
    let ticks = run["ticks_done"].as_u64()?;
    let count = |s: &Value, d: usize| {
        s["creatures"].as_f64().unwrap_or(0.0)
            * s["genes"]["diet"]["shares"][DIETS[d]].as_f64().unwrap_or(0.0)
    };
    let from = ticks as f64 * (1.0 - late);
    let late_snaps: Vec<&Value> =
        snaps.iter().filter(|s| s["tick"].as_f64().unwrap_or(0.0) >= from).collect();
    let mean = |f: &dyn Fn(&Value) -> f64| {
        if late_snaps.is_empty() {
            0.0
        } else {
            late_snaps.iter().map(|s| f(s)).sum::<f64>() / late_snaps.len() as f64
        }
    };
    let last = snaps.last()?;
    let by = &run["totals"]["by_diet"];
    let mut m = RunMetrics {
        seed,
        stop: run["stop"].as_str()?.to_string(),
        ticks,
        ms_per_tick: run["ms_per_tick"].as_f64().unwrap_or(0.0),
        pop_end: last["creatures"].as_f64().unwrap_or(0.0),
        pop_late: mean(&|s| s["creatures"].as_f64().unwrap_or(0.0)),
        pop_min: snaps
            .iter()
            .filter(|s| s["tick"].as_f64().unwrap_or(0.0) >= ticks as f64 * 0.1)
            .map(|s| s["creatures"].as_f64().unwrap_or(0.0))
            .fold(f64::INFINITY, f64::min),
        kills_by_carnivores: DIETS.iter().map(|d| by["kills"]["carnivore"][d].as_f64().unwrap_or(0.0)).sum(),
        pace: pace_of(run),
        ..RunMetrics::default()
    };
    if !m.pop_min.is_finite() {
        m.pop_min = m.pop_end;
    }
    for d in 0..4 {
        m.diet_late[d] = mean(&|s| count(s, d));
        m.diet_end[d] = count(last, d);
        m.born[d] = by["born"][DIETS[d]].as_f64().unwrap_or(0.0);
    }
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        if parts.next() == Some("METRIC")
            && let (Some(name), Some(value)) = (parts.next(), parts.next())
            && let Ok(v) = value.parse::<f64>()
        {
            m.extra.insert(name.to_string(), v);
        }
    }
    Some(m)
}

fn median(mut xs: Vec<f64>) -> f64 {
    if xs.is_empty() {
        return f64::NAN;
    }
    xs.sort_by(f64::total_cmp);
    let n = xs.len();
    if n % 2 == 1 { xs[n / 2] } else { (xs[n / 2 - 1] + xs[n / 2]) / 2.0 }
}

/// One row per variant: how many worlds ended on their own and survived, in how many each meat
/// diet held, the medians of the late shares and population, the deepest dip.
fn summarise(plan: &Plan, runs: &BTreeMap<String, Vec<RunMetrics>>, out: &Path) -> String {
    let extras: Vec<String> = {
        let mut names: Vec<String> = runs.values().flatten().flat_map(|m| m.extra.keys().cloned()).collect();
        names.sort();
        names.dedup();
        names
    };
    let mut md = String::from(
        "| variant | complete | survived | C holds | S holds | C+H coexist | C late % | S late % | O late % | pop late | pop min | C born | C kills | ms/tick | slowdown |",
    );
    for e in &extras {
        md += &format!(" {e} |");
    }
    md += "\n|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|";
    md += &"---|".repeat(extras.len());
    md += "\n";
    let mut csv = String::from(
        "variant,runs,complete,survived,c_holds,s_holds,coexist,c_late_share,s_late_share,o_late_share,pop_late,pop_min,c_born,c_kills,ms_per_tick,slowdown",
    );
    for e in &extras {
        csv += &format!(",{e}");
    }
    csv += "\n";
    for v in &plan.variants {
        let all = runs.get(&v.name).cloned().unwrap_or_default();
        let ok: Vec<&RunMetrics> = all.iter().filter(|m| m.complete()).collect();
        let n = plan.seeds.len();
        if ok.is_empty() {
            // nothing ended on its own: dashes, not NaN
            md += &format!("| {} | 0/{n} |{}\n", v.name, " – |".repeat(13 + extras.len()));
            csv += &format!("{},{n},0{}\n", v.name, ",".repeat(13 + extras.len()));
            continue;
        }
        let survived = ok.iter().filter(|m| m.pop_end > 0.0).count();
        let holds = |d| ok.iter().filter(|m| m.holds(d)).count();
        let coexist = ok.iter().filter(|m| m.holds(0) && m.holds(3)).count();
        let share = |d: usize| {
            100.0
                * median(
                    ok.iter()
                        .map(|m| if m.pop_late > 0.0 { m.diet_late[d] / m.pop_late } else { 0.0 })
                        .collect(),
                )
        };
        let pop_late = median(ok.iter().map(|m| m.pop_late).collect());
        let pop_min = ok.iter().map(|m| m.pop_min).fold(f64::INFINITY, f64::min);
        let c_born = median(ok.iter().map(|m| m.born[3]).collect());
        let c_kills = median(ok.iter().map(|m| m.kills_by_carnivores).collect());
        // the median run's tick rate, and the worst slowdown of any run (a boom or a stall)
        let ms = median(ok.iter().filter_map(|m| m.pace.map(|p| p.median)).collect());
        let slowdown = ok.iter().filter_map(|m| m.pace.map(|p| p.slowdown)).fold(f64::NAN, f64::max);
        let slow_mark = if slowdown >= SLOWDOWN_WARN { " ⚠" } else { "" };
        let extra: Vec<f64> = extras
            .iter()
            .map(|e| median(ok.iter().filter_map(|m| m.extra.get(e).copied()).collect()))
            .collect();
        md += &format!(
            "| {} | {}/{n} | {survived} | {} | {} | {coexist} | {:.1} | {:.1} | {:.1} | {:.0} | {:.0} | {:.0} | {:.0} | {ms:.2} | {slowdown:.1}×{slow_mark} |",
            v.name,
            ok.len(),
            holds(3),
            holds(2),
            share(3),
            share(2),
            share(1),
            pop_late,
            pop_min,
            c_born,
            c_kills
        );
        for x in &extra {
            md += &format!(" {x:.1} |");
        }
        md += "\n";
        csv += &format!(
            "{},{n},{},{survived},{},{},{coexist},{:.2},{:.2},{:.2},{:.0},{:.0},{:.0},{:.0},{ms:.3},{slowdown:.2}",
            v.name,
            ok.len(),
            holds(3),
            holds(2),
            share(3),
            share(2),
            share(1),
            pop_late,
            pop_min,
            c_born,
            c_kills
        );
        for x in &extra {
            csv += &format!(",{x:.2}");
        }
        csv += "\n";
    }
    let _ = fs::write(out.join("summary.csv"), &csv);
    let legend = "Medians over the worlds that ended on their own. \"holds\": at least 10 creatures and 1% of the \
                  world over the late window; \"coexist\": herbivores and carnivores both hold; \"pop min\": the \
                  lowest population after the first 10% of the run, over all worlds; \"ms/tick\": the median \
                  run's median lap (500 ticks); \"slowdown\": the worst lap over its run's median lap, of the \
                  slowest run (⚠ from 3×).\n\n";
    let _ = fs::write(out.join("summary.md"), format!("{legend}{md}"));
    md
}

fn write_runs_csv(runs: &BTreeMap<String, Vec<RunMetrics>>, out: &Path) {
    let mut csv = String::from(
        "variant,seed,stop,ticks,ms_per_tick,pace_median_ms,slowdown,slowest_at,pop_end,pop_late,pop_min",
    );
    for d in DIETS {
        csv += &format!(",{d}_late,{d}_end,{d}_born");
    }
    csv += ",kills_by_carnivores,extra\n";
    for (name, list) in runs {
        for m in list {
            csv += &format!(
                "{name},{},{},{},{:.2},{:.3},{:.2},{},{:.0},{:.1},{:.0}",
                m.seed,
                m.stop,
                m.ticks,
                m.ms_per_tick,
                m.pace.map_or(f64::NAN, |p| p.median),
                m.pace.map_or(f64::NAN, |p| p.slowdown),
                m.pace.map_or(0, |p| p.worst_at),
                m.pop_end,
                m.pop_late,
                m.pop_min
            );
            for d in 0..4 {
                csv += &format!(",{:.1},{:.0},{:.0}", m.diet_late[d], m.diet_end[d], m.born[d]);
            }
            let extra: Vec<String> = m.extra.iter().map(|(k, v)| format!("{k}={v}")).collect();
            csv += &format!(",{:.0},{}\n", m.kills_by_carnivores, extra.join(" "));
        }
    }
    let _ = fs::write(out.join("runs.csv"), csv);
}

fn collect(plan: &Plan, args: &Args) -> BTreeMap<String, Vec<RunMetrics>> {
    let mut runs: BTreeMap<String, Vec<RunMetrics>> = BTreeMap::new();
    for v in &plan.variants {
        for &seed in &plan.seeds {
            let job = Job { variant: v.clone(), seed };
            let (json, txt, _) = job_paths(&args.out, &job);
            let text = fs::read_to_string(&txt).unwrap_or_default();
            let m = read_json(&json)
                .and_then(|j| metrics_of(&j, &text, seed, args.late))
                .unwrap_or(RunMetrics { seed, stop: "missing".into(), ..RunMetrics::default() });
            runs.entry(v.name.clone()).or_default().push(m);
        }
    }
    runs
}

fn minutes(d: Duration) -> String {
    format!("{:.1} min", d.as_secs_f64() / 60.0)
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// One finished run as the progress window lists it.
struct Finished {
    variant: String,
    seed: u64,
    outcome: Outcome,
    /// The report's stop reason, when it wrote its JSON.
    stop: Option<String>,
    /// Its tick rate, when the report measured it.
    pace: Option<Pace>,
}

impl Finished {
    fn cut(&self) -> bool {
        match &self.outcome {
            Outcome::Killed(_) | Outcome::Failed(_) => true,
            _ => self.stop.as_deref().is_some_and(|s| s != "done" && s != "extinct"),
        }
    }

    /// Its worst lap was `SLOWDOWN_WARN` times slower than its median one.
    fn slow(&self) -> bool {
        self.pace.is_some_and(|p| p.slowdown >= SLOWDOWN_WARN)
    }

    fn what(&self) -> String {
        let slow = match self.pace {
            Some(p) if self.slow() => format!(", tick rate {:.1}x slower by tick {}", p.slowdown, p.worst_at),
            _ => String::new(),
        };
        self.outcome_text() + &slow
    }

    fn outcome_text(&self) -> String {
        match &self.outcome {
            Outcome::Reused => "reused".to_string(),
            Outcome::Exited(t) => {
                format!("{:.0} s, {}", t.as_secs_f64(), self.stop.as_deref().unwrap_or("?"))
            }
            Outcome::Killed(t) => format!("KILLED after {:.0} s", t.as_secs_f64()),
            Outcome::Failed(e) => format!("FAILED: {e}"),
            Outcome::Interrupted => "interrupted".to_string(),
        }
    }
}

/// What the progress window shows, rewritten to `OUT/progress.json` every second.
struct Progress {
    title: String,
    about: String,
    total: usize,
    jobs: usize,
    worst_case: Duration,
    started: Instant,
    started_unix: u64,
    finished: Vec<Finished>,
    running: Vec<(String, u64, Instant)>,
    /// Where the runs write their ticks, and a run's deadline, seconds.
    out: PathBuf,
    limit_s: u64,
    /// `running`, `paused`, `stopped` (by the user; the sweep ends) or `finished`.
    state: &'static str,
    done: bool,
}

/// «Tick of ticks» of each running run, in the order of `Progress::running`, when it has said.
type Ticks = Vec<Option<(u64, u64, f64)>>;

/// Share of its ticks a running run has done.
fn share_of(ticks: Option<(u64, u64, f64)>) -> Option<f64> {
    ticks.map(|(tick, total, _)| (tick as f64 / total as f64).min(1.0))
}

impl Progress {
    /// The running runs' ticks, read once per update (`Ticks`).
    fn ticks(&self) -> Ticks {
        self.running.iter().map(|r| tick_of(&self.out, &r.0, r.1)).collect()
    }

    /// The whole sweep's share done, the running runs' ticks included.
    fn share(&self, ticks: &Ticks) -> f64 {
        let partial: f64 = ticks.iter().filter_map(|&t| share_of(t)).sum();
        if self.total == 0 {
            0.0
        } else {
            ((self.finished.len() as f64 + partial) / self.total as f64).min(1.0)
        }
    }

    /// Seconds left: the queued runs and the unfinished part of the running ones, spread over the
    /// slots — but never less than the slowest running one still needs. A run's time is the mean
    /// of the runs that ran here, or, before the first ends, what the running ones' ticks promise.
    fn left(&self, ticks: &Ticks) -> Option<f64> {
        let ran: Vec<f64> = self
            .finished
            .iter()
            .filter_map(|f| match f.outcome {
                Outcome::Exited(t) | Outcome::Killed(t) => Some(t.as_secs_f64()),
                _ => None,
            })
            .collect();
        // (elapsed, share done) of the running runs that have said how far they are
        let told: Vec<(f64, f64)> = self
            .running
            .iter()
            .zip(ticks)
            .filter_map(|(r, &t)| share_of(t).map(|s| (r.2.elapsed().as_secs_f64(), s)))
            .filter(|&(_, s)| s >= 0.02)
            .collect();
        let avg = if !ran.is_empty() {
            ran.iter().sum::<f64>() / ran.len() as f64
        } else if !told.is_empty() {
            told.iter().map(|(t, s)| t / s).sum::<f64>() / told.len() as f64
        } else {
            return None;
        };
        let queued = self.total - self.finished.len() - self.running.len();
        let running: Vec<f64> = self
            .running
            .iter()
            .zip(ticks)
            .map(|(r, &ticks)| {
                let t = r.2.elapsed().as_secs_f64();
                match share_of(ticks).filter(|&s| s >= 0.02) {
                    Some(s) => t / s - t,
                    None => (avg - t).max(0.0),
                }
            })
            .collect();
        let work = queued as f64 * avg + running.iter().sum::<f64>();
        Some((work / self.jobs as f64).max(running.iter().copied().fold(0.0, f64::max)))
    }

    fn to_json(&self, plan: &Plan) -> Value {
        let ticks = self.ticks();
        // the running runs by variant: (seed, seconds, «tick of ticks» once it has said)
        type Run = (u64, f64, Option<(u64, u64, f64)>);
        let mut groups: Vec<(String, Vec<Run>)> = Vec::new();
        for ((name, seed, since), &told) in self.running.iter().zip(&ticks) {
            let run = (*seed, since.elapsed().as_secs_f64(), told);
            match groups.iter_mut().find(|g| &g.0 == name) {
                Some(g) => g.1.push(run),
                None => groups.push((name.clone(), vec![run])),
            }
        }
        let out = &self.out;
        let about =
            |name: &str| plan.variants.iter().find(|v| v.name == name).map_or("", |v| v.about.as_str());
        let last: Vec<Value> = self
            .finished
            .iter()
            .rev()
            .take(6)
            .map(|f| {
                json!({ "variant": f.variant, "seed": f.seed, "what": f.what(), "cut": f.cut(), "slow": f.slow() })
            })
            .collect();
        json!({
            "title": self.title,
            "about": self.about,
            "total": self.total,
            "done": self.finished.len(),
            "reused": self.finished.iter().filter(|f| matches!(f.outcome, Outcome::Reused)).count(),
            "cut": self.finished.iter().filter(|f| f.cut()).count(),
            "jobs": self.jobs,
            "elapsed_s": self.started.elapsed().as_secs_f64().round(),
            "left_s": self.left(&ticks).map(f64::round),
            "worst_case_s": self.worst_case.as_secs(),
            "started_unix": self.started_unix,
            "updated_unix": unix_now(),
            "progress": self.share(&ticks),
            "limit_s": self.limit_s,
            "running": groups
                .iter()
                .map(|(name, runs)| {
                    let seeds: Vec<u64> = runs.iter().map(|r| r.0).collect();
                    let longest = runs.iter().map(|r| r.1).fold(0.0, f64::max);
                    let told: Vec<(u64, u64, f64)> = runs.iter().filter_map(|r| r.2).collect();
                    let n = told.len() as u64;
                    let (tick, total) = told.iter().fold((0, 0), |a, t| (a.0 + t.0, a.1 + t.1));
                    json!({
                        "variant": name, "about": about(name), "seeds": seeds, "longest_s": longest.round(),
                        // the mean tick and length of the runs that have said
                        "tick": (n > 0).then(|| tick / n), "ticks": (n > 0).then(|| total / n),
                        // and each run apart, for the window's per-seed view
                        "runs": runs.iter().map(|(seed, s, told)| json!({
                            "seed": seed, "s": s.round(),
                            "tick": told.map(|t| t.0), "ticks": told.map(|t| t.1),
                            "ms": told.map(|t| t.2).filter(|&ms| ms > 0.0),
                        })).collect::<Vec<_>>(),
                    })
                })
                .collect::<Vec<_>>(),
            "last": last,
            "state": self.state,
            "finished": self.done,
            "summary": if self.done { Some(out.join("summary.md").display().to_string()) } else { None },
            "out": out.display().to_string(),
        })
    }

    /// Written aside and renamed over, so the window never reads half a file. A failed write (the
    /// window holding the file that very moment) is simply retried a second later.
    fn write(&self, plan: &Plan) {
        let tmp = self.out.join("progress.json.tmp");
        if fs::write(&tmp, self.to_json(plan).to_string()).is_ok() {
            let _ = fs::rename(&tmp, self.out.join("progress.json"));
        }
    }
}

/// Opens the progress window on `OUT/progress.json`, detached: closing it does not stop the sweep.
fn open_window(args: &Args) {
    let viewer = args.viewer.clone().or_else(|| {
        let me = std::env::current_exe().ok()?;
        Some(me.with_file_name(format!("life-progress{}", std::env::consts::EXE_SUFFIX)))
    });
    match viewer {
        Some(v) if v.is_file() => {
            let spawned = Command::new(&v)
                .arg(args.out.join("progress.json"))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            if let Err(e) = spawned {
                println!("progress window: {}: {e}", v.display());
            }
        }
        _ => println!(
            "progress window: life-progress not found next to this program \
             (cargo build -p life-app --release --bin life-progress); progress is in {}",
            args.out.join("progress.json").display()
        ),
    }
}

fn main() {
    let args = Args::parse();
    let fail = |e: String| -> ! {
        eprintln!("error: {e}");
        std::process::exit(2);
    };
    let text =
        fs::read_to_string(&args.plan).unwrap_or_else(|e| fail(format!("{}: {e}", args.plan.display())));
    let plan = parse_plan(&text).unwrap_or_else(|e| fail(e));
    if !(0.0..=1.0).contains(&args.late) || args.late == 0.0 {
        fail("--late is a share of the run, above 0 and at most 1".into());
    }
    fs::create_dir_all(&args.out).unwrap_or_else(|e| fail(format!("{}: {e}", args.out.display())));
    if args.summary_only {
        return report(&plan, &args);
    }
    let exe = args.exe.clone().unwrap_or_else(|| {
        let me = std::env::current_exe().unwrap_or_else(|e| fail(e.to_string()));
        me.with_file_name(format!("life-report{}", std::env::consts::EXE_SUFFIX))
    });
    if !exe.is_file() {
        fail(format!(
            "{} not found: build it (cargo build -p life-report --release) or pass --exe",
            exe.display()
        ));
    }
    let cpus = std::thread::available_parallelism().map_or(4, |n| n.get());
    let jobs_at_once = args.jobs.unwrap_or(cpus.saturating_sub(4).max(1)).max(1);
    let queue: VecDeque<Job> = plan
        .variants
        .iter()
        .flat_map(|v| plan.seeds.iter().map(|&seed| Job { variant: v.clone(), seed }))
        .collect();
    let total = queue.len();
    let worst_case = Duration::from_secs(total.div_ceil(jobs_at_once) as u64 * (args.seconds + args.grace));
    println!(
        "{} variants x {} seeds = {total} runs, {jobs_at_once} at a time; worst case {} (every run to its deadline)",
        plan.variants.len(),
        plan.seeds.len(),
        minutes(worst_case)
    );
    let title = args.plan.file_stem().map_or_else(|| "sweep".into(), |s| s.to_string_lossy().into_owned());
    let progress = Arc::new(Mutex::new(Progress {
        title,
        about: plan.about.clone(),
        total,
        jobs: jobs_at_once,
        worst_case,
        started: Instant::now(),
        started_unix: unix_now(),
        finished: Vec::new(),
        running: Vec::new(),
        out: args.out.clone(),
        limit_s: args.seconds,
        state: "running",
        done: false,
    }));
    // a pause or stop left over from an earlier sweep in the same folder must not stop this one
    let _ = fs::write(args.out.join("control.txt"), "run");
    let control = Arc::new(AtomicU8::new(RUN));
    progress.lock().expect("progress").write(&plan);
    if !args.no_window {
        open_window(&args);
    }
    let queue = Arc::new(Mutex::new(queue));
    let plan = Arc::new(plan);
    let args = Arc::new(args);
    let exe = Arc::new(exe);
    let workers: Vec<_> = (0..jobs_at_once)
        .map(|_| {
            let (queue, progress, plan, args, exe, control) =
                (queue.clone(), progress.clone(), plan.clone(), args.clone(), exe.clone(), control.clone());
            std::thread::spawn(move || {
                loop {
                    // paused: wait; stopped: leave
                    loop {
                        match control.load(Ordering::Relaxed) {
                            RUN => break,
                            STOP => return,
                            _ => std::thread::sleep(Duration::from_millis(300)),
                        }
                    }
                    let Some(job) = queue.lock().expect("queue").pop_front() else { break };
                    let started = Instant::now();
                    progress.lock().expect("progress").running.push((
                        job.variant.name.clone(),
                        job.seed,
                        started,
                    ));
                    let outcome = run_job(&exe, &plan, &job, &args, &control);
                    let json = job_paths(&args.out, &job).0;
                    let _ = fs::remove_file(json.with_extension("tick"));
                    let _ = fs::remove_file(json.with_extension("tick.tmp"));
                    let report = read_json(&json);
                    let stop =
                        report.as_ref().and_then(|j| j["runs"][0]["stop"].as_str().map(str::to_string));
                    let pace = report.as_ref().and_then(|j| pace_of(&j["runs"][0]));
                    let mut p = progress.lock().expect("progress");
                    p.running.retain(|r| !(r.0 == job.variant.name && r.1 == job.seed));
                    if matches!(outcome, Outcome::Interrupted) {
                        // back to the front of the queue: after the pause it runs first, anew
                        queue.lock().expect("queue").push_front(job);
                        continue;
                    }
                    p.finished.push(Finished {
                        variant: job.variant.name.clone(),
                        seed: job.seed,
                        outcome,
                        stop,
                        pace,
                    });
                    let left = p.left(&p.ticks()).map_or(String::new(), |s| {
                        format!(", about {} left", minutes(Duration::from_secs_f64(s)))
                    });
                    println!(
                        "[{}/{total}] {} seed {}: {} ({} elapsed{left})",
                        p.finished.len(),
                        job.variant.name,
                        job.seed,
                        p.finished.last().expect("just pushed").what(),
                        minutes(p.started.elapsed())
                    );
                }
            })
        })
        .collect();
    // The window's buttons land here. Each press is printed and logged to `OUT/events.log`, so
    // whoever watches the sweep's output (Claude, through a watcher on it) learns of it at once.
    let mut last_write = Instant::now() - Duration::from_secs(1);
    while !workers.iter().all(|w| w.is_finished()) {
        let wanted = read_control(&args.out);
        if wanted != control.load(Ordering::Relaxed) && control.load(Ordering::Relaxed) != STOP {
            control.store(wanted, Ordering::Relaxed);
            let mut p = progress.lock().expect("progress");
            p.state = ["running", "paused", "stopped"][wanted as usize];
            let what = match wanted {
                PAUSE => format!(
                    "PAUSED by the user in the window: {} of {total} runs done, {} running taken off to rerun",
                    p.finished.len(),
                    p.running.len()
                ),
                STOP => format!(
                    "STOPPED by the user in the window: {} of {total} runs done; the summary covers only them",
                    p.finished.len()
                ),
                _ => format!("RESUMED by the user in the window: {} of {total} runs done", p.finished.len()),
            };
            println!("{what}");
            let log = fs::OpenOptions::new().create(true).append(true).open(args.out.join("events.log"));
            let _ = log.and_then(|mut f| writeln!(f, "{} {what}", unix_now()));
            p.write(&plan);
        }
        if last_write.elapsed() >= Duration::from_secs(1) {
            progress.lock().expect("progress").write(&plan);
            last_write = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    for w in workers {
        w.join().expect("a worker");
    }
    report(&plan, &args);
    let mut p = progress.lock().expect("progress");
    p.done = true;
    if p.state != "stopped" {
        p.state = "finished";
    }
    p.write(&plan);
}

fn report(plan: &Plan, args: &Args) {
    let runs = collect(plan, args);
    write_runs_csv(&runs, &args.out);
    let cut: Vec<String> = runs
        .iter()
        .flat_map(|(name, list)| {
            list.iter().filter(|m| !m.complete()).map(move |m| format!("{name} s{} {}", m.seed, m.stop))
        })
        .collect();
    println!("\n{}", summarise(plan, &runs, &args.out));
    let slow: Vec<String> = runs
        .iter()
        .flat_map(|(name, list)| {
            list.iter().filter_map(move |m| {
                let p = m.pace?;
                (p.slowdown >= SLOWDOWN_WARN).then(|| {
                    format!(
                        "{name} s{} {:.1}x by tick {} ({:.2} ms median)",
                        m.seed, p.slowdown, p.worst_at, p.median
                    )
                })
            })
        })
        .collect();
    if !slow.is_empty() {
        println!("TICK RATE FELL {SLOWDOWN_WARN}x or more: {}", slow.join("; "));
    }
    if !cut.is_empty() {
        println!("not complete (left out of the medians): {}", cut.join("; "));
    }
    println!("summary: {}", args.out.join("summary.md").display());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plan_reads_seeds_args_and_variants() {
        let plan = parse_plan(
            "# shared\nseeds: 1 2\nargs: --scale 20 --ticks 100\n\nvariant control:\nvariant young: EXP_YOUNG=0.7 --rule carnivore_smell=2 # note\n",
        )
        .unwrap();
        assert_eq!(plan.seeds, [1, 2]);
        assert_eq!(plan.args, ["--scale", "20", "--ticks", "100"]);
        assert_eq!(plan.variants.len(), 2);
        assert!(plan.variants[0].env.is_empty() && plan.variants[0].args.is_empty());
        assert_eq!(plan.variants[1].env, [("EXP_YOUNG".to_string(), "0.7".to_string())]);
        assert_eq!(plan.variants[1].args, ["--rule", "carnivore_smell=2"]);
    }

    /// The top block describes the plan; a block describes the variants under it until
    /// `seeds:`/`args:` or the next block; a line's own comment describes that variant alone.
    #[test]
    fn comments_describe_the_plan_and_its_variants() {
        let plan = parse_plan(
            "# Carnivore bodies\n# on the user's world\nseeds: 1\nvariant control:\n\
             # a keen nose\nvariant a: --rule x=1\nvariant b: --rule x=2 # the keenest\nvariant c:\n\
             # a juvenile gut\n\nvariant d: E_X=1\nargs: --ticks 10\nvariant e:\n",
        )
        .unwrap();
        assert_eq!(plan.about, "Carnivore bodies on the user's world");
        let about: Vec<&str> = plan.variants.iter().map(|v| v.about.as_str()).collect();
        assert_eq!(about, ["", "a keen nose", "the keenest", "a keen nose", "a juvenile gut", ""]);
    }

    #[test]
    fn time_left_spreads_the_work_over_the_slots() {
        let mut p = Progress {
            title: String::new(),
            about: String::new(),
            total: 10,
            jobs: 2,
            worst_case: Duration::ZERO,
            started: Instant::now(),
            started_unix: 0,
            finished: Vec::new(),
            running: Vec::new(),
            out: PathBuf::from("nowhere"),
            limit_s: 300,
            state: "running",
            done: false,
        };
        assert_eq!(p.left(&p.ticks()), None, "nothing to go by before the first run ends");
        for (seed, outcome) in [(1, Outcome::Exited(Duration::from_secs(100))), (2, Outcome::Reused)] {
            p.finished.push(Finished {
                variant: "a".into(),
                seed,
                outcome,
                stop: Some("done".into()),
                pace: None,
            });
        }
        // 8 queued at 100 s each over 2 slots; reused runs do not count towards the mean
        let left = p.left(&p.ticks()).unwrap();
        assert!((left - 400.0).abs() < 1.0, "{left}");
        p.finished.push(Finished {
            variant: "a".into(),
            seed: 3,
            outcome: Outcome::Reused,
            stop: Some("deadline".into()),
            pace: None,
        });
        assert!(p.finished[2].cut() && !p.finished[1].cut());
    }

    /// A variant may be called `control`: its folder does not collide with the window's file, and
    /// a variant with no finished run gets dashes in the summary, not NaN.
    #[test]
    fn a_variant_named_control_and_one_without_runs() {
        let out = std::env::temp_dir().join(format!("life-sweep-test-{}", std::process::id()));
        fs::create_dir_all(&out).unwrap();
        fs::write(out.join("control.txt"), "pause").unwrap();
        assert_eq!(read_control(&out), PAUSE);
        let plan = parse_plan("seeds: 1\nvariant control:\n").unwrap();
        let job = Job { variant: plan.variants[0].clone(), seed: 1 };
        let dir = job_paths(&out, &job).0.parent().unwrap().to_path_buf();
        fs::create_dir_all(&dir).expect("the variant's folder is not the control file");
        let runs = BTreeMap::from([(
            "control".to_string(),
            vec![RunMetrics { seed: 1, stop: "missing".into(), ..RunMetrics::default() }],
        )]);
        let md = summarise(&plan, &runs, &out);
        assert!(md.contains("| control | 0/1 |") && !md.contains("NaN") && !md.contains("inf"), "{md}");
        assert_eq!(
            md.lines().nth(2).unwrap().matches('|').count(),
            md.lines().next().unwrap().matches('|').count()
        );
        fs::remove_dir_all(&out).unwrap();
    }

    /// Before any run ends, the running runs' ticks already give the share done and the time left;
    /// a run that has not said yet does not halve its variant's tick.
    #[test]
    fn ticks_of_running_runs_count() {
        let out = std::env::temp_dir().join(format!("life-sweep-ticks-{}", std::process::id()));
        fs::create_dir_all(out.join("a")).unwrap();
        fs::write(out.join("a").join("s1.tick"), "5000 20000 4.25").unwrap();
        let p = Progress {
            title: String::new(),
            about: String::new(),
            total: 4,
            jobs: 2,
            worst_case: Duration::ZERO,
            started: Instant::now(),
            started_unix: 0,
            finished: Vec::new(),
            running: vec![
                ("a".into(), 1, Instant::now() - Duration::from_secs(10)),
                ("a".into(), 2, Instant::now()),
            ],
            out: out.clone(),
            limit_s: 300,
            state: "running",
            done: false,
        };
        assert_eq!(tick_of(&out, "a", 1), Some((5000, 20000, 4.25)));
        assert_eq!(tick_of(&out, "a", 2), None);
        let ticks = p.ticks();
        assert_eq!(ticks, [Some((5000, 20000, 4.25)), None]);
        assert!((p.share(&ticks) - 0.0625).abs() < 1e-9, "a quarter of one run of four: {}", p.share(&ticks));
        // a quarter in 10 s: 40 s a run; 30 s left of it, 40 of the silent one and 2 queued at 40 s,
        // over 2 slots
        let left = p.left(&ticks).expect("the ticks promise a time");
        assert!((left - 75.0).abs() < 1.0, "{left}");
        let json = p.to_json(&parse_plan("seeds: 1 2\nvariant a:\n").unwrap());
        let run = &json["running"][0];
        assert_eq!(run["seeds"], json!([1, 2]));
        assert_eq!((run["tick"].as_u64(), run["ticks"].as_u64()), (Some(5000), Some(20000)), "{run}");
        // each seed apart: the silent one has no tick yet
        assert_eq!(
            (run["runs"][0]["tick"].as_u64(), run["runs"][0]["ms"].as_f64()),
            (Some(5000), Some(4.25))
        );
        assert_eq!((run["runs"][1]["seed"].as_u64(), run["runs"][1]["tick"].as_u64()), (Some(2), None));
        assert_eq!(json["limit_s"].as_u64(), Some(300));
        fs::remove_dir_all(&out).unwrap();
    }

    #[test]
    fn a_plan_may_not_touch_the_guards() {
        for bad in [
            "args: --seconds 9999",
            "variant a: --threads 8",
            "variant b: --seed 3",
            "variant c: --seconds=9999",
            "args: --progress=x.tick",
        ] {
            let err = parse_plan(&format!("seeds: 1\n{bad}\nvariant ok:\n")).unwrap_err();
            assert!(err.contains("set by the sweep"), "{bad}: {err}");
        }
        assert!(parse_plan("seeds: 1\nvariant a:\nvariant a:\n").unwrap_err().contains("twice"));
        assert!(parse_plan("variant a:\n").unwrap_err().contains("seeds"));
    }

    #[test]
    fn the_command_line_keeps_the_guards() {
        let plan = parse_plan("seeds: 3\nargs: --ticks 50\nvariant v: A_B=1 --rule x=2\n").unwrap();
        let job = Job { variant: plan.variants[0].clone(), seed: 3 };
        let line = command_line(Path::new("r.exe"), &plan, &job, 120, Path::new("o.json"));
        assert_eq!(line[0], "A_B=1");
        let rest = line[1..].join(" ");
        assert_eq!(
            rest,
            "r.exe --seed 3 --ticks 50 --rule x=2 --max-work 1e15 --seconds 120 --threads 1 --json o.json \
             --progress o.tick"
        );
        // a budget of the plan's own, in either spelling, is not doubled
        for own in ["--max-work 1e14", "--max-work=1e14"] {
            let plan = parse_plan(&format!("seeds: 3\nvariant v: {own}\n")).unwrap();
            let job = Job { variant: plan.variants[0].clone(), seed: 3 };
            let line = command_line(Path::new("r.exe"), &plan, &job, 120, Path::new("o.json"));
            assert_eq!(line.iter().filter(|t| flag_name(t) == "--max-work").count(), 1, "{own}: {line:?}");
        }
    }

    /// A rebuilt report is another build: its results are not reused under the same command line.
    #[test]
    fn a_rebuilt_report_is_not_reused() {
        let dir = std::env::temp_dir().join(format!("life-sweep-stamp-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("r.exe");
        fs::write(&exe, "one").unwrap();
        let first = build_stamp(&exe);
        assert_eq!(build_stamp(&exe), first, "the same build");
        fs::write(&exe, "a longer one").unwrap();
        assert_ne!(build_stamp(&exe), first, "rebuilt");
        assert_eq!(build_stamp(&dir.join("gone.exe")), "unknown build");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn metrics_read_late_means_holds_and_extra_lines() {
        let snap = |tick: u64, n: f64, c: f64| {
            serde_json::json!({
                "tick": tick, "creatures": n,
                "genes": { "diet": { "shares": { "herbivore": 1.0 - c, "omnivore": 0.0, "scavenger": 0.0, "carnivore": c } } }
            })
        };
        let json = serde_json::json!({ "runs": [{
            "stop": "done", "ticks_done": 1000, "ms_per_tick": 2.5,
            "pace": [[250, 2.0], [500, 2.0], [750, 2.5], [1000, 8.0]],
            "totals": { "by_diet": { "born": { "carnivore": 40 }, "kills": { "carnivore": { "herbivore": 7, "carnivore": 1 } } } },
            "snapshots": [snap(0, 100.0, 0.0), snap(500, 50.0, 0.0), snap(800, 1000.0, 0.05), snap(1000, 3000.0, 0.01)],
        }]});
        let m = metrics_of(&json, "noise\nMETRIC carnivore_adults 12\nMETRIC bad x\n", 4, 0.25).unwrap();
        assert!(m.complete());
        assert_eq!(m.pop_late, 2000.0);
        assert_eq!(m.pop_min, 50.0);
        assert_eq!(m.diet_late[3], 40.0); // (50 + 30) / 2
        assert!(m.holds(3) && m.holds(0) && !m.holds(2));
        assert_eq!(m.born[3], 40.0);
        assert_eq!(m.kills_by_carnivores, 8.0);
        assert_eq!(m.extra.get("carnivore_adults"), Some(&12.0));
        // the tick rate: median lap 2.25 ms, the last lap 8 ms — a slowdown to flag
        assert_eq!(m.pace, Some(Pace { median: 2.25, slowdown: 8.0 / 2.25, worst_at: 1000 }));
        let f = Finished {
            variant: "a".into(),
            seed: 4,
            outcome: Outcome::Exited(Duration::from_secs(60)),
            stop: Some("done".into()),
            pace: m.pace,
        };
        assert!(f.slow() && !f.cut(), "slow is not cut");
        assert_eq!(f.what(), "60 s, done, tick rate 3.6x slower by tick 1000");
        assert_eq!(pace_of(&serde_json::json!({})), None, "an older report without laps");
        assert_eq!(m.extra.len(), 1);
    }
}

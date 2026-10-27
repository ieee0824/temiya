//! Compiled matcher strategies and independent per-object search storage.
use super::{RuntimeError, RuntimeResult};
use regex_automata::{
    Input,
    meta::Regex,
    nfa::thompson::{
        NFA,
        backtrack::{self, BoundedBacktracker},
        pikevm::{self, PikeVM},
    },
    util::captures::{Captures, GroupInfo},
};
use regex_syntax::hir::Hir;
use std::cell::RefCell;

#[derive(Clone)]
pub(super) enum Matcher {
    Meta(Regex),
    Direct { fast: BoundedBacktracker },
}

pub(super) enum Scratch {
    Meta,
    Direct(Box<DirectScratch>),
}

pub(super) struct DirectScratch {
    fast: backtrack::Cache,
    linear: Option<Box<LinearScratch>>,
}

struct LinearScratch {
    vm: PikeVM,
    cache: pikevm::Cache,
}

// These immutable builders only wrap already compiled NFAs. Their internal
// compilers never receive a pattern, so retained initialization is input independent.
thread_local! {
    static BACKTRACK_BUILDER: backtrack::Builder = BoundedBacktracker::builder();
    static LINEAR_BUILDER: pikevm::Builder = PikeVM::builder();
}

const SMALL_SOURCE_UNITS: usize = 4096;
const SMALL_NFA_BYTES: usize = 128 * 1024;
const FULL_NFA_BYTES: usize = 10 * (1 << 20);

fn compiler(limit: usize) -> regex_automata::nfa::thompson::Compiler {
    let mut compiler = NFA::compiler();
    compiler.configure(
        NFA::config()
            .utf8(true)
            .reverse(false)
            .shrink(false)
            .nfa_size_limit(Some(limit)),
    );
    compiler
}

thread_local! {
    // Only short ASCII source enters this compiler. Its intermediate-state budget
    // is small; failures discard all retained scratch before the full-limit retry.
    static SMALL_COMPILER: RefCell<regex_automata::nfa::thompson::Compiler> = RefCell::new(compiler(SMALL_NFA_BYTES));
    #[cfg(test)]
    static SMALL_FALLBACKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn compile_nfa(
    hir: &Hir,
    source_units: usize,
) -> Result<NFA, regex_automata::nfa::thompson::BuildError> {
    if source_units <= SMALL_SOURCE_UNITS {
        let result = SMALL_COMPILER.with(|slot| {
            let mut small = slot.borrow_mut();
            let result = small.build_from_hir(hir);
            if result.is_err() {
                *small = compiler(SMALL_NFA_BYTES);
            }
            result
        });
        if result.is_ok() {
            return result;
        }
        #[cfg(test)]
        SMALL_FALLBACKS.with(|count| count.set(count.get() + 1));
    }
    compiler(FULL_NFA_BYTES).build_from_hir(hir)
}

impl Matcher {
    pub(super) fn from_hir(hir: &Hir, source_units: usize) -> RuntimeResult<Self> {
        let nfa = compile_nfa(hir, source_units)
            .map_err(|error| RuntimeError::syntax_error(error.to_string()))?;
        let fast = BACKTRACK_BUILDER
            .with(|builder| builder.build_from_nfa(nfa))
            .map_err(|error| RuntimeError::syntax_error(error.to_string()))?;
        Ok(Self::Direct { fast })
    }

    pub(super) fn create_captures(&self) -> Captures {
        match self {
            Self::Meta(re) => re.create_captures(),
            Self::Direct { fast, .. } => fast.create_captures(),
        }
    }

    pub(super) fn create_scratch(&self) -> Scratch {
        match self {
            Self::Meta(_) => Scratch::Meta,
            Self::Direct { fast, .. } => Scratch::Direct(Box::new(DirectScratch {
                fast: fast.create_cache(),
                linear: None,
            })),
        }
    }

    pub(super) fn search(&self, input: &Input<'_>, captures: &mut Captures, scratch: &mut Scratch) {
        match (self, scratch) {
            (Self::Meta(re), Scratch::Meta) => re.search_captures(input, captures),
            (Self::Direct { fast }, Scratch::Direct(scratch)) => {
                // The bounded engine can reject subjects larger than its visited
                // bitmap. The same NFA then runs on the linear, unbounded-length VM.
                if fast.try_search(&mut scratch.fast, input, captures).is_err() {
                    let linear = scratch.linear.get_or_insert_with(|| {
                        // Both builders validate the same look set; the bounded
                        // builder already accepted this immutable NFA.
                        let vm = LINEAR_BUILDER
                            .with(|builder| builder.build_from_nfa(fast.get_nfa().clone()))
                            .expect("NFA already validated by bounded engine");
                        let cache = vm.create_cache();
                        Box::new(LinearScratch { vm, cache })
                    });
                    linear.vm.search(&mut linear.cache, input, captures);
                }
            }
            _ => unreachable!("matcher and per-object scratch must agree"),
        }
    }

    pub(super) fn group_info(&self) -> &GroupInfo {
        match self {
            Self::Meta(re) => re.group_info(),
            Self::Direct { fast, .. } => fast.get_nfa().group_info(),
        }
    }

    pub(super) fn template_memory_usage(&self) -> usize {
        match self {
            Self::Meta(re) => re
                .memory_usage()
                .saturating_add(re.create_cache().memory_usage()),
            // These engines share the same immutable NFA. Their search scratch
            // lives only in JavaScript objects, never in cache templates.
            Self::Direct { fast, .. } => fast.get_nfa().memory_usage(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(miri))]
    fn small_compiler_budget_retries_full_limit_and_keeps_old_programs_valid() {
        let make = |source: &str| {
            Matcher::from_hir(
                &super::super::ascii_hir::parse(source).unwrap(),
                source.len(),
            )
        };
        let original = make("(a)").unwrap();
        let before = SMALL_FALLBACKS.with(|count| count.get());
        let large = make("a{16384}").unwrap();
        assert!(large.template_memory_usage() > SMALL_NFA_BYTES);
        assert!(SMALL_FALLBACKS.with(|count| count.get()) > before);
        assert_eq!(
            make("a{1000000000}").err().unwrap().kind(),
            crate::runtime::RuntimeErrorKind::SyntaxError
        );
        let after_failure = make("(b)").unwrap();
        for (matcher, subject) in [(original, "a"), (after_failure, "b")] {
            let mut captures = matcher.create_captures();
            matcher.search(
                &Input::new(subject),
                &mut captures,
                &mut matcher.create_scratch(),
            );
            assert_eq!(captures.get_group(0).unwrap().range(), 0..1);
            assert_eq!(captures.get_group(1).unwrap().range(), 0..1);
        }
        let before = SMALL_FALLBACKS.with(|count| count.get());
        make(&"a".repeat(SMALL_SOURCE_UNITS + 1)).unwrap();
        assert_eq!(SMALL_FALLBACKS.with(|count| count.get()), before);
    }

    #[test]
    fn bounded_capacity_exhaustion_uses_linear_fallback_and_resets_captures() {
        let mut matcher =
            Matcher::from_hir(&super::super::ascii_hir::parse("(a+)(b)?").unwrap(), 8).unwrap();
        if let Matcher::Direct { fast, .. } = &mut matcher {
            *fast = BoundedBacktracker::builder()
                .configure(BoundedBacktracker::config().visited_capacity(1))
                .build_from_nfa(fast.get_nfa().clone())
                .unwrap();
        }
        let mut scratch = matcher.create_scratch();
        let mut captures = matcher.create_captures();
        matcher.search(&Input::new("aaab"), &mut captures, &mut scratch);
        assert_eq!(captures.get_group(0).unwrap().range(), 0..4);
        assert_eq!(captures.get_group(2).unwrap().range(), 3..4);
        assert!(matches!(scratch, Scratch::Direct(ref s) if s.linear.is_some()));
        matcher.search(&Input::new("aaa"), &mut captures, &mut scratch);
        assert_eq!(captures.get_group(0).unwrap().range(), 0..3);
        assert_eq!(captures.get_group(2), None);
        matcher.search(&Input::new("x"), &mut captures, &mut scratch);
        assert!(!captures.is_match());
    }
}

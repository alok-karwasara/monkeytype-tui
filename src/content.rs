mod catalog;

use crate::engine::{
    Mode, Settings,
    prompt::{Prompt, PromptError},
};
use rand::{Rng, SeedableRng, TryRng, rngs::SmallRng, seq::SliceRandom};
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exercise {
    pub id: String,
    pub version: i64,
    pub pool: String,
    pub pool_version: i64,
    pub language: Option<String>,
}
impl Exercise {
    pub fn words() -> Self {
        Self {
            id: "english-common".into(),
            version: 1,
            pool: "words".into(),
            pool_version: 1,
            language: None,
        }
    }
    pub fn fixture() -> Self {
        Self {
            id: "fixture".into(),
            version: 1,
            pool: "code".into(),
            pool_version: 1,
            language: Some("Rust".into()),
        }
    }
}
#[derive(Debug, Clone)]
pub struct Prepared {
    pub meta: Exercise,
    pub source: String,
}

pub fn words() -> &'static [&'static str] {
    static WORDS: OnceLock<Vec<&'static str>> = OnceLock::new();
    WORDS.get_or_init(|| {
        include_str!("content/words.txt")
            .split_whitespace()
            .collect()
    })
}
pub fn catalog() -> &'static [Prepared] {
    static CODE: OnceLock<Vec<Prepared>> = OnceLock::new();
    CODE.get_or_init(|| {
        [
            ("Rust", catalog::RUST),
            ("Python", catalog::PYTHON),
            ("JavaScript", catalog::JAVASCRIPT),
        ]
        .into_iter()
        .flat_map(|(language, rows)| {
            rows.iter().enumerate().map(move |(i, source)| Prepared {
                meta: Exercise {
                    id: format!("{}-{:03}", language.to_lowercase(), i + 1),
                    version: 1,
                    pool: "code".into(),
                    pool_version: 1,
                    language: Some(language.into()),
                },
                source: (*source).into(),
            })
        })
        .collect()
    })
}

#[derive(Debug)]
pub struct Selector {
    pub rng: SmallRng,
    word_deck: Vec<usize>,
    code_deck: Vec<usize>,
    last_word: Option<usize>,
    last_code: Option<usize>,
}
impl Default for Selector {
    fn default() -> Self {
        Self::new()
    }
}
impl Selector {
    pub fn new() -> Self {
        Self::with_rng(rand::make_rng())
    }
    pub fn try_new() -> Result<Self, std::io::Error> {
        let mut seed = [0; 32];
        let result = rand::rngs::SysRng
            .try_fill_bytes(&mut seed)
            .map(|()| seed)
            .map_err(std::io::Error::other);
        Self::from_entropy(result)
    }
    pub fn from_entropy(seed: Result<[u8; 32], std::io::Error>) -> Result<Self, std::io::Error> {
        Ok(Self::with_rng(SmallRng::from_seed(seed?)))
    }
    pub fn session_id(&mut self) -> String {
        // Deduplication IDs, never authentication/security tokens.
        let mut bytes = [0; 16];
        self.rng.fill_bytes(&mut bytes);
        uuid::Builder::from_random_bytes(bytes)
            .into_uuid()
            .to_string()
    }
    pub fn seeded(seed: u64) -> Self {
        Self::with_rng(SmallRng::seed_from_u64(seed))
    }
    fn with_rng(rng: SmallRng) -> Self {
        Self {
            rng,
            word_deck: Vec::new(),
            code_deck: Vec::new(),
            last_word: None,
            last_code: None,
        }
    }
    fn draw(deck: &mut Vec<usize>, last: Option<usize>, n: usize, rng: &mut SmallRng) -> usize {
        draw_index(deck, last, n, rng).unwrap_or(0)
    }
    pub fn words(&mut self) -> String {
        (0..32)
            .map(|_| {
                let i = Self::draw(
                    &mut self.word_deck,
                    self.last_word,
                    words().len(),
                    &mut self.rng,
                );
                self.last_word = Some(i);
                words()[i]
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
    pub fn code(&mut self) -> Prepared {
        let i = Self::draw(
            &mut self.code_deck,
            self.last_code,
            catalog().len(),
            &mut self.rng,
        );
        self.last_code = Some(i);
        catalog()[i].clone()
    }
}

pub fn draw_index(
    deck: &mut Vec<usize>,
    last: Option<usize>,
    n: usize,
    rng: &mut SmallRng,
) -> Option<usize> {
    if n == 0 {
        return None;
    }
    if deck.is_empty() {
        *deck = (0..n).collect();
        deck.shuffle(rng);
        if n > 1 && deck.last().copied() == last {
            deck.swap(0, n - 1);
        }
    }
    deck.pop()
}

// Compatibility helpers for callers/tests that deliberately require a seeded fixture.
pub fn word_chunk(sequence: usize, chunk: usize) -> String {
    let mut selector = Selector::seeded(
        (sequence as u64)
            .wrapping_mul(65537)
            .wrapping_add(chunk as u64),
    );
    selector.words()
}
pub fn prepare(settings: Settings, sequence: usize) -> Result<Prompt, PromptError> {
    let mut selector = Selector::seeded(sequence as u64);
    match settings.mode {
        Mode::Words => Prompt::new(&selector.words(), Mode::Words),
        Mode::Code => Prompt::new(&selector.code().source, Mode::Code),
    }
}

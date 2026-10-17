use crate::engine::{
    Mode, Settings,
    prompt::{Prompt, PromptError},
};

// Original exercises written for this project; no external corpus is downloaded.
const WORDS: &[&str] = &[
    "the", "quiet", "river", "flows", "under", "an", "open", "sky", "build", "small", "things",
    "with", "care", "and", "give", "each", "idea", "time", "to", "grow", "light", "falls",
    "across", "green", "fields", "while", "birds", "return", "home", "every", "step", "can",
    "make", "a", "clear", "path", "through", "new", "places", "read", "write", "learn", "think",
    "move", "find", "keep", "simple", "words", "close", "warm", "sun", "bright", "stars", "deep",
    "water", "fresh", "air", "soft", "rain", "start", "again", "look", "ahead", "take", "your",
    "next", "turn", "at", "ease", "focus", "comes", "from", "steady", "practice", "not", "speed",
    "alone", "today",
];

const SNIPPETS: &[&str] = &[
    "fn main() {\n    let count = 3;\n    println!(\"count: {}\", count);\n}",
    "fn total(values: &[i32]) -> i32 {\n    let mut sum = 0;\n    for value in values {\n        sum += value;\n    }\n    sum\n}",
    "def greet(name):\n    message = \"hello, \" + name\n    return message\n\nprint(greet(\"world\"))",
    "const numbers = [1, 2, 3];\nconst doubled = numbers.map((n) => n * 2);\nconsole.log(doubled);",
];

pub fn word_chunk(sequence: usize, chunk: usize) -> String {
    (0..32)
        .map(|i| {
            WORDS[(sequence
                .wrapping_mul(13)
                .wrapping_add(chunk.wrapping_mul(32))
                .wrapping_add(i))
            .wrapping_mul(37)
                % WORDS.len()]
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn prepare(settings: Settings, sequence: usize) -> Result<Prompt, PromptError> {
    match settings.mode {
        Mode::Words => Prompt::new(&word_chunk(sequence, 0), Mode::Words),
        Mode::Code => Prompt::new(SNIPPETS[sequence % SNIPPETS.len()], Mode::Code),
    }
}

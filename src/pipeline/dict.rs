/*
 * @file dict.rs
 * @author doraemon-hub-art (1660219734@qq.com)
 * @brief Offline pinyin-to-word lookup over the bundled Rime dictionary
 * @date 2026-09-19
 *
 * @copyright Copyright (c) 2026
 */

use std::collections::HashMap;

use tracing::{debug, info, warn};

// Per-file logger id, same scheme as in main.rs.
const LOG_ID: &str = "[PipelineDict]";

/// The bundled dictionary, embedded at compile time: no data file has to be shipped next to
/// the binary or located at run time.
const DICT_SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/pinyin-simp/pinyin_simp.dict.yaml"
));

/// How many candidates a lookup hands back in this first version.
const CANDIDATE_LIMIT: usize = 3;

/// One candidate for a pinyin string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The Chinese word, for example 测试.
    pub word: String,
    /// Relative weight taken from the dictionary; the higher, the better ranked.
    pub weight: u32,
}

/// Pinyin to Chinese words. Built once at startup and read-only afterwards, so it needs no lock.
pub struct Dict {
    /// Key: pinyin with all whitespace removed and lower-cased. Value: candidates ordered by
    /// descending weight, ties keeping the order they have in the dictionary file.
    entries: HashMap<String, Vec<Candidate>>,
}

impl Dict {
    /// Parses the embedded dictionary.
    pub fn load() -> Self {
        let mut entries: HashMap<String, Vec<Candidate>> = HashMap::new();
        let mut malformed = 0_usize;
        let mut dropped = 0_usize;

        // The file is a YAML header followed by tab separated 词 / 拼音 / 权重 rows; the header
        // ends with a lone "..." line.
        let rows = DICT_SOURCE
            .lines()
            .skip_while(|line| line.trim() != "...")
            .skip(1);

        for line in rows {
            let line = line.trim_end();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let mut fields = line.split('\t');
            let (word, pinyin, weight) = match (fields.next(), fields.next(), fields.next()) {
                (Some(word), Some(pinyin), Some(weight)) if fields.next().is_none() => {
                    (word, pinyin, weight)
                }
                _ => {
                    malformed += 1;
                    continue;
                }
            };

            let weight: u32 = match weight.trim().parse() {
                Ok(weight) => weight,
                Err(_) => {
                    malformed += 1;
                    continue;
                }
            };

            // Zero-weight rows are rare glyph variants; they only add noise to candidates.
            if weight == 0 {
                dropped += 1;
                continue;
            }

            entries
                .entry(normalize(pinyin))
                .or_default()
                .push(Candidate {
                    word: word.to_string(),
                    weight,
                });
        }

        // Sorting is stable, so equal weights keep their file order: the ranking is
        // reproducible and never depends on hash map iteration order.
        for candidates in entries.values_mut() {
            candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.weight));
        }

        let words: usize = entries.values().map(Vec::len).sum();
        if malformed > 0 {
            warn!(target: LOG_ID, "Skipped {malformed} malformed line(s) in the bundled dictionary.");
        }
        info!(target: LOG_ID, "Loaded {words} entries / {} keys, {dropped} zero-weight rows dropped.", entries.len());

        Self { entries }
    }

    /// Looks up a pinyin string such as `ce shi` or `ceshi`: whitespace and case are ignored.
    ///
    /// Returns at most [`CANDIDATE_LIMIT`] candidates, highest weight first, and an empty
    /// vector when the dictionary has no entry for that pinyin.
    pub fn lookup(&self, pinyin: &str) -> Vec<Candidate> {
        let key = normalize(pinyin);
        match self.entries.get(&key) {
            Some(candidates) => candidates.iter().take(CANDIDATE_LIMIT).cloned().collect(),
            None => {
                debug!(target: LOG_ID, "No entry for {key:?}.");
                Vec::new()
            }
        }
    }

    /// Number of distinct pinyin keys; only used for diagnostics.
    pub fn key_count(&self) -> usize {
        self.entries.len()
    }
}

/// Removes every whitespace character and lower-cases the rest, so `ce shi`, `CESHI` and
/// `ceshi` all end up as the same key.
fn normalize(input: &str) -> String {
    input
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

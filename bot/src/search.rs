//! Rust port of the site's search (quartz/util/search.ts and quartz/util/searchCorrection.ts).
//! Keep the ranking in step with the TypeScript so everything agrees.

use std::collections::{HashMap, HashSet};

use serde::Deserialize;

/// A word's pages, best match first; the first `named` have the word in their title or path.
#[derive(Deserialize)]
struct Postings {
    named: usize,
    pages: Vec<String>,
}

/// How much a correction counts compared to an exact match.
const TYPO_WEIGHT: f64 = 0.3;
const PREFIX_WEIGHT: f64 = 0.2;
const MAX_CORRECTIONS: usize = 5;
/// A half-typed word that happens to be a real word counts for less than its completions.
const UNFINISHED_WEIGHT: f64 = 0.5;

pub struct SearchIndex {
    postings: HashMap<String, Postings>,
}

#[derive(Default)]
struct Rank {
    matched: u32,
    named: u32,
    score: f64,
}

impl SearchIndex {
    /// Builds the index from the site's shard files
    pub fn from_shards<'a>(shards: impl IntoIterator<Item = &'a str>) -> serde_json::Result<Self> {
        let mut postings = HashMap::new();
        for shard in shards {
            postings.extend(serde_json::from_str::<HashMap<String, Postings>>(shard)?);
        }
        Ok(Self { postings })
    }

    /// Page slugs for `query`, best first.
    /// With `last_word_unfinished` (still typing), the last word also matches longer words it's the start of.
    pub fn search(&self, query: &str, last_word_unfinished: bool) -> Vec<&str> {
        let mut seen = HashSet::new();
        let mut words = search_terms(query);
        words.retain(|word| seen.insert(word.clone()));
        let Some(last) = words.len().checked_sub(1) else {
            return Vec::new();
        };

        let mut ranked: HashMap<&str, Rank> = HashMap::new();
        for (index, word) in words.iter().enumerate() {
            let exact = self.postings.get(word);
            let unfinished = last_word_unfinished && index == last;
            let prefixes = index == last && (unfinished || exact.is_none());

            // The word itself if indexed, plus corrections when it isn't or might be unfinished
            let mut alternatives: Vec<(&Postings, f64)> = Vec::new();
            if let Some(postings) = exact {
                alternatives.push((postings, if unfinished { UNFINISHED_WEIGHT } else { 1.0 }));
            }
            if prefixes || exact.is_none() {
                alternatives.extend(self.corrections(word, prefixes, exact.is_none()));
            }

            // A page counts once per query word, through its best alternative; rarity covers all
            // alternatives so a rare correction doesn't beat a common one
            let total_pages: usize = alternatives
                .iter()
                .map(|(postings, _)| postings.pages.len())
                .sum();
            let rarity = 1.0 / ((total_pages + 1) as f64).log2();
            let mut best: HashMap<&str, (f64, bool)> = HashMap::new();
            for (postings, weight) in &alternatives {
                for (position, slug) in postings.pages.iter().enumerate() {
                    let score = weight * rarity / (position + 1) as f64;
                    if score > best.get(slug.as_str()).map_or(0.0, |(best, _)| *best) {
                        best.insert(slug, (score, position < postings.named));
                    }
                }
            }
            for (slug, (score, named)) in best {
                let rank = ranked.entry(slug).or_default();
                rank.matched += 1;
                rank.named += u32::from(named);
                rank.score += score;
            }
        }

        // Matching more query words, especially in the title or path, multiplies the score
        let mut results: Vec<(&str, f64)> = ranked
            .into_iter()
            .map(|(slug, rank)| {
                let named = f64::from(1 + rank.named);
                (
                    slug,
                    rank.score * f64::from(rank.matched * rank.matched) * (named * named),
                )
            })
            .collect();
        results.sort_by(|left, right| right.1.total_cmp(&left.1).then_with(|| left.0.cmp(right.0)));
        results.into_iter().map(|(slug, _)| slug).collect()
    }

    /// Indexed words close to `word`: with `typos`, within one typo (two for words over six letters);
    /// with `prefixes`, words that start with it. Closer and more common words first.
    fn corrections(&self, word: &str, prefixes: bool, typos: bool) -> Vec<(&Postings, f64)> {
        let max_edits = match word.len() {
            4..=6 if typos => 1,
            7.. if typos => 2,
            _ => 0,
        };
        let mut candidates: Vec<(f64, &str, &Postings, f64)> = Vec::new();
        for (candidate, postings) in &self.postings {
            if candidate == word {
                continue;
            }
            let commonness = ((postings.pages.len() + 1) as f64).log2() / 10.0;
            if prefixes && word.len() >= 2 && candidate.starts_with(word) {
                let order = (candidate.len() - word.len()) as f64 - commonness;
                candidates.push((order, candidate, postings, PREFIX_WEIGHT));
            } else if max_edits > 0 {
                let edits = edit_distance(word.as_bytes(), candidate.as_bytes(), max_edits);
                if edits <= max_edits {
                    let order = edits as f64 * 10.0 - commonness;
                    candidates.push((order, candidate, postings, TYPO_WEIGHT / edits as f64));
                }
            }
        }
        candidates
            .sort_by(|left, right| left.0.total_cmp(&right.0).then_with(|| left.1.cmp(right.1)));
        candidates.truncate(MAX_CORRECTIONS);
        candidates
            .into_iter()
            .map(|(_, _, postings, weight)| (postings, weight))
            .collect()
    }
}

/// Splits a query into words. A lone symbol like `&&` becomes its page path name.
/// A trailing `()` is dropped so `..()` finds the `..` proc (but `()` on its own stays the `()` operator).
fn search_terms(text: &str) -> Vec<String> {
    let is_word_char = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_';
    let mut terms = Vec::new();
    for chunk in text.to_lowercase().split_whitespace() {
        if chunk.chars().any(is_word_char) {
            terms.extend(
                chunk
                    .split(|c| !is_word_char(c))
                    .filter(|word| !word.is_empty())
                    .map(String::from),
            );
            continue;
        }
        let symbol = match chunk.strip_suffix("()") {
            Some(rest) if chunk.chars().count() > 2 => rest,
            _ => chunk,
        };
        let name: String = symbol.chars().filter_map(symbol_name).collect();
        if !name.is_empty() {
            terms.push(name);
        }
    }
    terms
}

/// Same names the scraper uses for symbols in page paths (TEXT_REPLACEMENTS in scraper/src/main.rs).
fn symbol_name(symbol: char) -> Option<&'static str> {
    Some(match symbol {
        '.' => "dot",
        '<' => "less",
        '>' => "greater",
        '%' => "modulo",
        '?' => "query",
        '&' => "amp",
        '~' => "tilde",
        '|' => "vert",
        '!' => "exclaim",
        ':' => "colon",
        '*' => "asterisk",
        '^' => "caret",
        '=' => "equals",
        '+' => "plus",
        '-' => "minus",
        '/' => "slash",
        '(' => "leftparen",
        ')' => "rightparen",
        '[' => "leftsquare",
        ']' => "rightsquare",
        _ => return None,
    })
}

/// Optimal string alignment distance (Levenshtein plus swapped neighbours), giving up past `max`.
fn edit_distance(a: &[u8], b: &[u8], max: usize) -> usize {
    if a.len().abs_diff(b.len()) > max {
        return max + 1;
    }
    let mut previous2: Vec<usize> = Vec::new();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut row = vec![i];
        let mut row_best = i;
        for j in 1..=b.len() {
            let mut distance = (previous[j] + 1)
                .min(row[j - 1] + 1)
                .min(previous[j - 1] + usize::from(a[i - 1] != b[j - 1]));
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                distance = distance.min(previous2[j - 2] + 1);
            }
            row.push(distance);
            row_best = row_best.min(distance);
        }
        if row_best > max {
            return max + 1;
        }
        previous2 = std::mem::replace(&mut previous, row);
    }
    previous[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbols_become_page_path_names() {
        assert_eq!(search_terms("&& operator"), ["ampamp", "operator"]);
        assert_eq!(
            search_terms("..() ?[] ()"),
            [
                "dotdot",
                "queryleftsquarerightsquare",
                "leftparenrightparen"
            ]
        );
        assert_eq!(
            search_terms("list.Add(x) /proc/for"),
            ["list", "add", "x", "proc", "for"]
        );
    }

    #[test]
    fn edit_distance_counts_swaps_as_one_and_stops_past_max() {
        assert_eq!(edit_distance(b"istpye", b"istype", 2), 1);
        assert_eq!(edit_distance(b"ranom", b"random", 1), 1);
        assert_eq!(edit_distance(b"sleep", b"spawn", 1), 2);
    }
}

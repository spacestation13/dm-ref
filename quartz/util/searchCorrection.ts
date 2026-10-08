/* dm-ref EDIT: file added for dm-ref */
// Typo and prefix correction for search words. Used by searchPages in ./search.ts; the build
// writes the word list to static/search/words.json.

// Every indexed word, with how many pages it's on
export type SearchWords = Record<string, number>

// How much a corrected word counts compared to an exact match. A one-typo correction counts
// TYPO_WEIGHT, a two-typo one half that.
const TYPO_WEIGHT = 0.3
const PREFIX_WEIGHT = 0.2
const MAX_CORRECTIONS = 5

// Optimal string alignment distance (Levenshtein plus swapped neighbours), giving up past `max`
function editDistance(a: string, b: string, max: number): number {
  if (Math.abs(a.length - b.length) > max) return max + 1
  let previous2: number[] = []
  let previous = Array.from({ length: b.length + 1 }, (_, j) => j)
  for (let i = 1; i <= a.length; i++) {
    const row = [i]
    let rowBest = i
    for (let j = 1; j <= b.length; j++) {
      let distance = Math.min(
        previous[j] + 1,
        row[j - 1] + 1,
        previous[j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1),
      )
      if (i > 1 && j > 1 && a[i - 1] === b[j - 2] && a[i - 2] === b[j - 1]) {
        distance = Math.min(distance, previous2[j - 2] + 1)
      }
      row.push(distance)
      rowBest = Math.min(rowBest, distance)
    }
    if (rowBest > max) return max + 1
    previous2 = previous
    previous = row
  }
  return previous[b.length]
}

// Indexed words close to `word`: with `typos`, within one typo (two for words over six letters);
// with `prefixes`, words that start with it (from two letters). Closer and more common words first.
export function correctWord(
  word: string,
  words: SearchWords,
  { prefixes, typos }: { prefixes: boolean; typos: boolean },
): { word: string; weight: number }[] {
  const maxEdits = !typos || word.length <= 3 ? 0 : word.length <= 6 ? 1 : 2
  const candidates: { word: string; weight: number; order: number }[] = []
  for (const [candidate, pages] of Object.entries(words)) {
    if (candidate === word) continue
    const commonness = Math.log2(pages + 1) / 10
    if (prefixes && word.length >= 2 && candidate.startsWith(word)) {
      const order = candidate.length - word.length - commonness
      candidates.push({ word: candidate, weight: PREFIX_WEIGHT, order })
    } else if (maxEdits > 0) {
      const edits = editDistance(word, candidate, maxEdits)
      if (edits <= maxEdits) {
        const order = edits * 10 - commonness
        candidates.push({ word: candidate, weight: TYPO_WEIGHT / edits, order })
      }
    }
  }
  return candidates
    .sort((left, right) => left.order - right.order || (left.word < right.word ? -1 : 1)) // Ties break alphabetically
    .slice(0, MAX_CORRECTIONS)
    .map(({ word, weight }) => ({ word, weight }))
}

/* dm-ref EDIT: file added for dm-ref */
// Search shared by the site's search bar and the MCP worker (mcp-worker/src/content.ts).
// The build writes one JSON shard per hash bucket to static/search/; both sides rank from those.

import { correctWord, type SearchWords } from "./searchCorrection"
export type { SearchWords }

// For each word: its pages, best match first, and how many at the front have it in their name
export type SearchPostings = { named: number; pages: string[] }

export function searchShard(word: string): string {
  let hash = 0
  for (const character of word) {
    hash = (Math.imul(hash, 31) + character.charCodeAt(0)) >>> 0
  }
  return String(hash % 256).padStart(3, "0")
}

// Same names the scraper uses for symbols in page paths (see TEXT_REPLACEMENTS in
// scraper/src/main.rs), so searching `&&` finds `operator/ampamp`.
// prettier-ignore
const SYMBOL_NAMES: Record<string, string> = {
  ".": "dot", "<": "less", ">": "greater", "%": "modulo", "?": "query",
  "&": "amp", "~": "tilde", "|": "vert", "!": "exclaim", ":": "colon",
  "*": "asterisk", "^": "caret", "=": "equals", "+": "plus", "-": "minus", "/": "slash",
  "(": "leftparen", ")": "rightparen", "[": "leftsquare", "]": "rightsquare",
}

// Splits a query into words. A lone symbol like `&&` becomes its path name; a trailing `()` is
// dropped so `..()` finds the `..` proc (but `()` on its own stays the `()` operator).
export function searchTerms(text: string): string[] {
  return text
    .toLowerCase()
    .split(/\s+/)
    .flatMap((chunk) => {
      if (!/^[^\sa-z0-9_]+$/.test(chunk)) {
        return chunk.match(/[a-z0-9_]+/g) ?? []
      }
      const symbol = chunk.length > 2 ? chunk.replace(/\(\)$/, "") : chunk
      const name = [...symbol].map((character) => SYMBOL_NAMES[character] ?? "").join("")
      return name ? [name] : []
    })
}

// Ranks page slugs for a query. `loadShard` returns the parsed shard file for a bucket;
// `loadWords` returns the word list, used to correct words that aren't in the index. With
// `lastWordUnfinished` (someone is still typing it), the last word also matches longer words.
export async function searchPages(
  query: string,
  loadShard: (shard: string) => Promise<Record<string, SearchPostings>>,
  loadWords: () => Promise<SearchWords>,
  { lastWordUnfinished = false } = {},
): Promise<string[]> {
  const postings = async (word: string) => (await loadShard(searchShard(word)))[word]
  const words = [...new Set(searchTerms(query))]

  // Each query word becomes one or more indexed words: itself if indexed, plus corrections when
  // it isn't indexed or might be unfinished. A half-typed word that happens to be a real word
  // (`ran`) counts for less, so it doesn't drown out what it's the start of (`rand`).
  const matches = await Promise.all(
    words.map(async (word, index) => {
      const exact = await postings(word)
      const unfinished = lastWordUnfinished && index === words.length - 1
      const prefixes = index === words.length - 1 && (unfinished || !exact)
      const alternatives = exact ? [{ word, weight: unfinished ? 0.5 : 1 }] : []
      if (prefixes || !exact) {
        alternatives.push(...correctWord(word, await loadWords(), { prefixes, typos: !exact }))
      }
      return Promise.all(
        alternatives.map(async ({ word, weight }) => ({ weight, ...(await postings(word))! })),
      )
    }),
  )

  // A page scores higher the rarer the word and the nearer the top of that word's list it sits.
  // Matching more query words, especially in the title or path, multiplies the score. A corrected
  // query word counts once per page, through its best-scoring correction, and its rarity covers
  // all its corrections so a rare correction (`slept` for `sle`) doesn't beat a common one.
  const ranked = new Map<string, { matched: number; named: number; score: number }>()
  for (const alternatives of matches) {
    const best = new Map<string, { score: number; named: boolean }>()
    const totalPages = alternatives.reduce((total, { pages }) => total + pages.length, 0)
    const rarity = 1 / Math.log2(totalPages + 1)
    for (const { weight, named, pages } of alternatives) {
      pages.forEach((slug, position) => {
        const score = (weight * rarity) / (position + 1)
        if (score > (best.get(slug)?.score ?? 0)) best.set(slug, { score, named: position < named })
      })
    }
    for (const [slug, { score, named }] of best) {
      const entry = ranked.get(slug) ?? { matched: 0, named: 0, score: 0 }
      entry.matched += 1
      entry.named += named ? 1 : 0
      entry.score += score
      ranked.set(slug, entry)
    }
  }
  return Array.from(ranked, ([slug, { matched, named, score }]) => ({
    slug,
    rank: score * matched ** 2 * (1 + named) ** 2,
  }))
    .sort((left, right) => right.rank - left.rank || (left.slug < right.slug ? -1 : 1)) // Ties break by slug
    .map(({ slug }) => slug)
}

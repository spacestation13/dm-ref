import { searchPages, type SearchPostings, type SearchWords } from "../../quartz/util/search"

export type Env = {
  STATIC_BASE_URL: string
}

export function canonicalUrl(slug: string): string {
  return `https://ref.dm-lang.org/${slug.replace(/\/index$/, "")}`
}

export async function getSearchResults(env: Env, query: string): Promise<string[]> {
  const loadJson = async <T>(path: string): Promise<T> => {
    const response = await getCached(env, path)
    return response.ok ? await response.json() as T : {} as T
  }
  // The word list is only fetched when a query has a word that isn't in the index
  let words: Promise<SearchWords> | undefined
  return searchPages(
    query,
    (shard) => loadJson<Record<string, SearchPostings>>(`search/${shard}.json`),
    () => (words ??= loadJson<SearchWords>("search/words.json")),
  )
}

export type PageSummary = { title: string; snippet: string }

export async function getPages(env: Env): Promise<Record<string, PageSummary>> {
  const response = await getCached(env, "mcp/pages.json")
  return response.ok ? await response.json() as Record<string, PageSummary> : {}
}

async function getCached(env: Env, path: string): Promise<Response> {
  const url = new URL(path, env.STATIC_BASE_URL).toString()
  const cache = caches.default
  const cacheKey = new Request(url)
  const cached = await cache.match(cacheKey)
  if (cached) {
    return cached
  }

  const response = await fetch(url)
  if (!response.ok) {
    return response
  }

  const cachedResponse = new Response(response.body, response)
  cachedResponse.headers.set("Cache-Control", `public, max-age=${86400}`) // 1 day
  await cache.put(cacheKey, cachedResponse.clone())
  return cachedResponse
}
import { Root } from "hast"
import { GlobalConfiguration } from "../../cfg"
import { getDate } from "../../components/Date"
import { escapeHTML } from "../../util/escape"
import { FilePath, FullSlug, SimpleSlug, joinSegments, simplifySlug } from "../../util/path"
import { QuartzEmitterPlugin } from "../types"
import { toHtml } from "hast-util-to-html"
import { write } from "./helpers"
import { i18n } from "../../i18n"
/* dm-ref EDIT */
import { searchKeywords } from "./searchKeywords"
import { SearchPostings, SearchWords, searchShard } from "../../util/search"

export type ContentIndexMap = Map<FullSlug, ContentDetails>
export type ContentDetails = {
  slug: FullSlug
  filePath: FilePath
  title: string
  links: SimpleSlug[]
  tags: string[]
  /* dm-ref EDIT: page text is only kept during the build; the site gets it from searchText.json */
  content?: string
  richContent?: string
  date?: Date
  description?: string
}

interface Options {
  enableSiteMap: boolean
  enableRSS: boolean
  rssLimit?: number
  rssFullHtml: boolean
  rssSlug: string
  includeEmptyFiles: boolean
}

const defaultOptions: Options = {
  enableSiteMap: true,
  enableRSS: true,
  rssLimit: 10,
  rssFullHtml: false,
  rssSlug: "index",
  includeEmptyFiles: true,
}

function generateSiteMap(cfg: GlobalConfiguration, idx: ContentIndexMap): string {
  const base = cfg.baseUrl ?? ""
  const createURLEntry = (slug: SimpleSlug, content: ContentDetails): string => `<url>
    <loc>https://${joinSegments(base, encodeURI(slug))}</loc>
    ${content.date && `<lastmod>${content.date.toISOString()}</lastmod>`}
  </url>`
  const urls = Array.from(idx)
    .map(([slug, content]) => createURLEntry(simplifySlug(slug), content))
    .join("")
  return `<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml">${urls}</urlset>`
}

function generateRSSFeed(cfg: GlobalConfiguration, idx: ContentIndexMap, limit?: number): string {
  const base = cfg.baseUrl ?? ""

  const createURLEntry = (slug: SimpleSlug, content: ContentDetails): string => `<item>
    <title>${escapeHTML(content.title)}</title>
    <link>https://${joinSegments(base, encodeURI(slug))}</link>
    <guid>https://${joinSegments(base, encodeURI(slug))}</guid>
    <description>${content.richContent ?? content.description}</description>
    <pubDate>${content.date?.toUTCString()}</pubDate>
  </item>`

  const items = Array.from(idx)
    .sort(([_, f1], [__, f2]) => {
      if (f1.date && f2.date) {
        return f2.date.getTime() - f1.date.getTime()
      } else if (f1.date && !f2.date) {
        return -1
      } else if (!f1.date && f2.date) {
        return 1
      }

      return f1.title.localeCompare(f2.title)
    })
    .map(([slug, content]) => createURLEntry(simplifySlug(slug), content))
    .slice(0, limit ?? idx.size)
    .join("")

  return `<?xml version="1.0" encoding="UTF-8" ?>
<rss version="2.0">
    <channel>
      <title>${escapeHTML(cfg.pageTitle)}</title>
      <link>https://${base}</link>
      <description>${!!limit ? i18n(cfg.locale).pages.rss.lastFewNotes({ count: limit }) : i18n(cfg.locale).pages.rss.recentNotes} on ${escapeHTML(
        cfg.pageTitle,
      )}</description>
      <generator>Quartz -- quartz.jzhao.xyz</generator>
      ${items}
    </channel>
  </rss>`
}

/* dm-ref EDIT: search scoring, used by the site search bar and the MCP worker */
const searchWords = (text: string) => text.toLowerCase().match(/[a-z0-9_]+/g) ?? []

// How well each word matches a page. Words in the page's title, keywords or path count far more
// than words in the text, and count double if they're in both the path and the title.
// `json_encode` also counts as `json` and `encode`. Text matches use BM25 for length normalization.
const NAME_SCORE = 1000
function tokenScores(page: ContentDetails, averageLength: number): Map<string, number> {
  const words = searchWords(page.content ?? "")
  const counts = new Map<string, number>()
  for (const word of words) {
    counts.set(word, (counts.get(word) ?? 0) + 1)
  }
  const lengthFactor = 1.2 * (0.5 + (0.5 * words.length) / averageLength)
  const scores = new Map<string, number>()
  for (const [word, count] of counts) {
    scores.set(word, (count * 2.2) / (count + lengthFactor))
  }
  const keywords = searchKeywords[page.slug] ?? []
  for (const name of [page.slug, [page.title, ...keywords].join(" ")]) {
    for (const word of new Set(searchWords(name).flatMap((word) => [word, ...word.split("_")]))) {
      scores.set(word, (scores.get(word) ?? 0) + NAME_SCORE)
    }
  }
  return scores
}

/* dm-ref EDIT: search index, split into shards by searchShard */
function createSearchIndex(index: ContentIndexMap): Map<string, Map<string, SearchPostings>> {
  let totalLength = 0
  for (const page of index.values()) {
    totalLength += searchWords(page.content ?? "").length
  }
  const postings = new Map<string, { slug: string; score: number }[]>()
  for (const [slug, page] of index) {
    for (const [token, score] of tokenScores(page, totalLength / index.size)) {
      const pages = postings.get(token) ?? []
      pages.push({ slug, score })
      postings.set(token, pages)
    }
  }

  const searchIndex = new Map<string, Map<string, SearchPostings>>()
  for (const [token, pages] of postings) {
    pages.sort((left, right) => right.score - left.score)
    const shard = searchShard(token)
    const tokens = searchIndex.get(shard) ?? new Map()
    tokens.set(token, {
      named: pages.filter((page) => page.score >= NAME_SCORE).length,
      pages: pages.map((page) => page.slug),
    })
    searchIndex.set(shard, tokens)
  }
  return searchIndex
}

export const ContentIndex: QuartzEmitterPlugin<Partial<Options>> = (opts) => {
  opts = { ...defaultOptions, ...opts }
  return {
    name: "ContentIndex",
    async *emit(ctx, content) {
      const cfg = ctx.cfg.configuration
      const linkIndex: ContentIndexMap = new Map()
      for (const [tree, file] of content) {
        const slug = file.data.slug!
        const date = getDate(ctx.cfg.configuration, file.data) ?? new Date()
        if (opts?.includeEmptyFiles || (file.data.text && file.data.text !== "")) {
          linkIndex.set(slug, {
            slug,
            filePath: file.data.relativePath!,
            title: file.data.frontmatter?.title!,
            links: file.data.links ?? [],
            tags: file.data.frontmatter?.tags ?? [],
            /* dm-ref EDIT: drop embedded base64 images (`data:` URIs) so they aren't indexed as words */
            content: (file.data.text ?? "").replace(/data:[^\s)]*/g, ""),
            richContent: opts?.rssFullHtml
              ? escapeHTML(toHtml(tree as Root, { allowDangerousHtml: true }))
              : undefined,
            date: date,
            description: file.data.description ?? "",
          })
        }
      }

      if (opts?.enableSiteMap) {
        yield write({
          ctx,
          content: generateSiteMap(cfg, linkIndex),
          slug: "sitemap" as FullSlug,
          ext: ".xml",
        })
      }

      if (opts?.enableRSS) {
        yield write({
          ctx,
          content: generateRSSFeed(cfg, linkIndex, opts.rssLimit),
          slug: (opts?.rssSlug ?? "index") as FullSlug,
          ext: ".xml",
        })
      }

      /* dm-ref EDIT: page text goes to its own file so the explorer and graph don't wait on it */
      yield write({
        ctx,
        content: JSON.stringify(
          Object.fromEntries(Array.from(linkIndex, ([slug, page]) => [slug, page.content])),
        ),
        slug: joinSegments("static", "searchText") as FullSlug,
        ext: ".json",
      })

      const fp = joinSegments("static", "contentIndex") as FullSlug
      const simplifiedIndex = Object.fromEntries(
        Array.from(linkIndex).map(([slug, content]) => {
          // remove description and from content index as nothing downstream
          // actually uses it. we only keep it in the index as we need it
          // for the RSS feed
          delete content.description
          delete content.date
          /* dm-ref EDIT: page text lives in searchText.json */
          return [slug, { ...content, content: undefined }]
        }),
      )

      yield write({
        ctx,
        content: JSON.stringify(simplifiedIndex),
        slug: fp,
        ext: ".json",
      })

      /* dm-ref EDIT: search shards, and the word list used to correct typos */
      const words: SearchWords = {}
      for (const [shard, tokens] of createSearchIndex(linkIndex)) {
        for (const [token, { pages }] of tokens) {
          words[token] = pages.length
        }
        yield write({
          ctx,
          content: JSON.stringify(Object.fromEntries(tokens)),
          slug: joinSegments("static", "search", shard) as FullSlug,
          ext: ".json",
        })
      }
      yield write({
        ctx,
        content: JSON.stringify(words),
        slug: joinSegments("static", "search", "words") as FullSlug,
        ext: ".json",
      })

      /* dm-ref EDIT: titles and 120-character snippets for MCP results, so agents can tell pages
         apart (proc/del vs datum/proc/Del) and often answer without opening one */
      const snippet = (text: string) => {
        const flat = text
          .replace(/!\[[^\]]*\]\([^)]*\)/g, "")
          .replace(/\s+/g, " ")
          .trim()
        if (flat.length <= 120) return flat
        const wordEnd = flat.lastIndexOf(" ", 120)
        return flat.slice(0, wordEnd > 72 ? wordEnd : 120) + "…"
      }
      yield write({
        ctx,
        content: JSON.stringify(
          Object.fromEntries(
            Array.from(linkIndex, ([slug, page]) => [
              slug,
              { title: page.title.replaceAll("\\", ""), snippet: snippet(page.content ?? "") },
            ]),
          ),
        ),
        slug: joinSegments("static", "mcp", "pages") as FullSlug,
        ext: ".json",
      })
    },
    externalResources: (ctx) => {
      if (opts?.enableRSS) {
        return {
          additionalHead: [
            <link
              rel="alternate"
              type="application/rss+xml"
              title="RSS Feed"
              href={`https://${ctx.cfg.configuration.baseUrl}/index.xml`}
            />,
          ],
        }
      }
    },
  }
}

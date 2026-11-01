/* dm-ref EDIT: file added */
// A markdown version of every page (`<slug>.md`)
// The page's text without the site junk around it, with absolute links.
import { FileTrieNode } from "../../util/fileTrie"
import { simplifySlug, splitAnchor } from "../../util/path"
import { QuartzEmitterPlugin } from "../types"
import { write } from "./helpers"

type Page = { slug: string; title: string; filePath: string; version?: string; source: string }

export const MarkdownVersion: QuartzEmitterPlugin = () => ({
  name: "MarkdownVersion",
  async *emit(ctx, content) {
    const base = `https://${ctx.cfg.configuration.baseUrl}`
    const trie = new FileTrieNode<Page>([])
    for (const [, file] of content) {
      trie.add({
        slug: file.data.slug!,
        title: String(file.data.frontmatter?.title).replaceAll("\\", ""),
        filePath: file.data.filePath!,
        version: file.data.frontmatter?.byond_version as string | undefined,
        source: String(file.value),
      })
    }
    trie.sort((a, b) => a.slug.localeCompare(b.slug))
    const link = (node: FileTrieNode<Page>) => `- [${node.data?.title}](${base}/${node.slug}.md)`

    // `/atom/var/vis_flags#x` -> `<base>/atom/var/vis_flags.md#x`, `/image` -> `<base>/image/index.md`
    const resolve = (href: string) => {
      const [path, anchor] = splitAnchor(href)
      const node = trie.findNode(path.split("/").filter(Boolean))
      return node ? `${base}/${node.slug}.md${anchor}` : `${base}${href}`
    }

    for (const [slug, node] of trie.entries()) {
      if (!node.data) continue
      const { title, version, source } = node.data
      const lines = [`# ${title}`, "", `Source: ${new URL(simplifySlug(slug), `${base}/`)}`]
      if (version) lines.push(`Added in BYOND ${version}.`)
      lines.push("", body(source, resolve))
      if (node.children.length) lines.push("", "## Pages", "", ...node.children.map(link))
      yield write({ ctx, slug, ext: ".md", content: lines.join("\n") + "\n" })
    }
  },
})

function body(source: string, resolve: (href: string) => string): string {
  let inCode = false
  return source
    .replace(/^\+\+\+[\s\S]*?\+\+\+/, "") // drop the frontmatter
    .split("\n")
    .map((line) => {
      if (line.trimStart().startsWith("```")) {
        inCode = !inCode
        return line
      }
      if (inCode) return line.trimEnd()
      return line
        .replaceAll("\\_", "_") // scraper escapes underscores, agents don't need that
        .replaceAll(/!\[\]\(data:[^)]*\)/g, "(image omitted)") // base64 images are just noise
        .replaceAll(/\]\((\/[^)]*)\)/g, (_, href) => `](${resolve(href)})`) // site links -> full .md urls
    })
    .join("\n")
    .trim()
}

import { McpServer } from "@modelcontextprotocol/server"
import { z } from "zod"
import { canonicalUrl, getPages, getSearchResults, type Env } from "./content"

export function createServer(env: Env): McpServer {
  const server = new McpServer({ name: "dm-reference", version: "1.0.0" })

  server.registerTool(
    "search_dm_ref",
    {
      description: "Search the BYOND Dream Maker language reference. Use DM names, symbols, or a few keywords. Returns snippets and page URLs.",
      inputSchema: z.object({ query: z.string().min(1).max(200) }),
    },
    async ({ query }) => {
      const [slugs, pages] = await Promise.all([getSearchResults(env, query), getPages(env)])
      const matches = slugs.slice(0, 10).map((slug) => ({
        title: pages[slug]?.title,
        path: `/${slug}`,
        url: canonicalUrl(slug),
        snippet: pages[slug]?.snippet,
      }))

      return { content: [{ type: "text", text: JSON.stringify(matches) }] }
    },
  )

  return server
}
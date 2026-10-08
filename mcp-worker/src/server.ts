import { McpServer } from "@modelcontextprotocol/server"
import { z } from "zod"
import { canonicalUrl, getSearchResults, getTitles, type Env } from "./content"

export function createServer(env: Env): McpServer {
  const server = new McpServer({ name: "dm-reference", version: "1.0.0" })

  server.registerTool(
    "search_dm_ref",
    {
      description: "Search the BYOND Dream Maker language reference. Use a few distinctive keywords.",
      inputSchema: z.object({ query: z.string().min(1).max(200) }),
    },
    async ({ query }) => {
      const [slugs, titles] = await Promise.all([getSearchResults(env, query), getTitles(env)])
      const matches = slugs.slice(0, 10).map((slug) => ({
        title: titles[slug],
        path: `/${slug}`,
        url: canonicalUrl(slug),
      }))

      return { content: [{ type: "text", text: JSON.stringify(matches) }] }
    },
  )

  return server
}
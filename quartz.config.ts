import { existsSync } from "fs"
import { QuartzConfig } from "./quartz/cfg"
import * as Plugin from "./quartz/plugins"
import path from "path"

/**
 * Quartz 4 Configuration
 *
 * See https://quartz.jzhao.xyz/configuration for more information.
 */
const config: QuartzConfig = {
	configuration: {
		pageTitle: "DM Reference",
		pageTitleSuffix: " | DM Reference",
		enableSPA: true,
		enablePopovers: true,
		analytics: null,
		locale: "en-US",
		baseUrl: "ref.dm-lang.org",
		builtFor: "%BYOND VERSION%",
		ignorePatterns: ["private", "templates", ".obsidian"],
		defaultDateType: "modified",
		theme: {
			fontOrigin: "local",
			cdnCaching: true,
			typography: {
				header: "Verdana",
				body: "Verdana",
				code: "Menlo",
			},
			colors: {
				lightMode: {
					light: "#faf8f8",
					lightgray: "#e5e5e5",
					gray: "#b8b8b8",
					darkgray: "#4e4e4e",
					dark: "#2b2b2b",
					secondary: "blue",
					tertiary: "#84a59d",
					highlight: "rgba(143, 159, 169, 0.15)",
					textHighlight: "#fff23688",
				},
				darkMode: {
					light: "#161618",
					lightgray: "#393639",
					gray: "#646464",
					darkgray: "#d4d4d4",
					dark: "#ebebec",
					secondary: "#99f",
					tertiary: "#84a59d",
					highlight: "rgba(143, 159, 169, 0.15)",
					textHighlight: "#b3aa0288",
				},
			},
		},
	},
	plugins: {
		transformers: [
			Plugin.FrontMatter({ delimiters: "+++", language: "toml" }),
			Plugin.SyntaxHighlighting({
				theme: {
					light: "github-light",
					dark: "github-dark",
				},
				keepBackground: false,
			}),
			Plugin.ObsidianFlavoredMarkdown({ enableInHtmlEmbed: false, parseTags: false }),
			Plugin.GitHubFlavoredMarkdown(),
			Plugin.TableOfContents(),
			Plugin.CrawlLinks({ markdownLinkResolution: "absolute" }),
			Plugin.Description(),
			Plugin.Latex({ renderEngine: "katex" }),
		],
		filters: [Plugin.RemoveDrafts()],
		emitters: [
			Plugin.AliasRedirects(),
			Plugin.ComponentResources(),
			Plugin.ContentPage(),
			Plugin.FolderPage(),
			Plugin.TagPage(),
			Plugin.ContentIndex({
				enableSiteMap: true,
				enableRSS: true,
			}),
			Plugin.Assets(),
			Plugin.Static(),
			Plugin.Favicon(),
			Plugin.NotFoundPage(),
			// Comment out CustomOgImages to speed up build time
			Plugin.CustomOgImages({ colorScheme: "darkMode" }),
		],
	},
}

export default config

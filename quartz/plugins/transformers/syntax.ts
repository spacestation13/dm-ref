import { createHighlighter } from "shiki"
import { QuartzTransformerPlugin } from "../types"
import rehypePrettyCode, { Options as CodeOptions, Theme as CodeTheme } from "rehype-pretty-code"
import { readFile, readFileSync } from "fs"

interface Theme extends Record<string, CodeTheme> {
  light: CodeTheme
  dark: CodeTheme
}

interface Options {
  theme?: Theme
  keepBackground?: boolean
}

const defaultOptions: Options = {
  theme: {
    light: "github-light",
    dark: "github-dark",
  },
  keepBackground: false,
}

export const SyntaxHighlighting: QuartzTransformerPlugin<Partial<Options>> = (userOpts) => {
  const opts: CodeOptions = { ...defaultOptions, ...userOpts }

  /* dm-ref EDIT: Shiki calls DM `dream-maker`; let pages just write ```dm */
  opts.defaultLang = { inline: "dm" }
  opts.getHighlighter = (options) =>
    createHighlighter({ ...options, langAlias: { dm: "dream-maker" } })

  return {
    name: "SyntaxHighlighting",
    htmlPlugins() {
      return [[rehypePrettyCode, opts]]
    },
  }
}

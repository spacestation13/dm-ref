import { Root as HTMLRoot } from "hast"
import { QuartzTransformerPlugin } from "../types"

export interface Options {
  descriptionLength: number
  maxDescriptionLength: number
  replaceExternalLinks: boolean
}

const defaultOptions: Options = {
  descriptionLength: 150,
  maxDescriptionLength: 300,
  replaceExternalLinks: true,
}

const urlRegex = new RegExp(
  /(https?:\/\/)?(?<domain>([\da-z\.-]+)\.([a-z\.]{2,6})(:\d+)?)(?<path>[\/\w\.-]*)(\?[\/\w\.=&;-]*)?/,
  "g",
)

export const Description: QuartzTransformerPlugin<Partial<Options>> = (userOpts) => {
  const opts = { ...defaultOptions, ...userOpts }
  return {
    name: "Description",
    htmlPlugins() {
      return [
        () => {
          return async (_tree: HTMLRoot, file) => {
            let frontMatterDescription = file.data.frontmatter?.description
            let text = (file.value as string).replace(/\+\+\+.*\+\+\+/gs, "").replaceAll(/\[.*]\(([a-zA-Z0-9/]*)\)/g, "$1").replaceAll(/[>#`-]/g, "").replaceAll("\\_", "_")

            if (opts.replaceExternalLinks) {
              frontMatterDescription = frontMatterDescription?.replace(
                urlRegex,
                "$<domain>" + "$<path>",
              )
              text = text.replace(urlRegex, "$<domain>" + "$<path>")
            }

            if (frontMatterDescription) {
              file.data.description = frontMatterDescription
              file.data.text = text
              return
            }

            /* dm-ref EDIT: describe the page by its prose, with link labels rather than paths and
               without the See also list. Signature-only pages fall back to Format + Returns. */
            const clean = (markdown: string) =>
              markdown
                .replaceAll("\\_", "_")
                .replaceAll(/!\[[^\]]*\]\([^)]*\)/g, "")
                .replaceAll(/\[([^\]]*)\]\([^)]*\)/g, "$1")
                .replaceAll(/^\s*(?:#+|>|-)\s?/gm, "")
                .replaceAll(/`|\*\*/g, "")
            const headers: Record<string, string[]> = file.data.frontmatter?.headers ?? {}
            let desc = clean(
              (file.value as string).replace(/\+\+\+.*?\+\+\+/s, "").replace(/^#+ See also$[\s\S]*/m, ""),
            )
            for (const [header, parts] of Object.entries(headers)) {
              desc = desc.replace(header, "")
              for (const part of parts) {
                desc = desc.replace(clean(part), "")
              }
            }
            if (!desc.trim()) {
              const format = headers.Format?.map(clean).join(" or ")
              const returns = headers.Returns?.map(clean).join(" ")
              desc = [format, returns].filter(Boolean).join(": ")
            }
            if (opts.replaceExternalLinks) {
              desc = desc.replace(urlRegex, "$<domain>" + "$<path>")
            }

            const sentences = desc.replace(/\s+/g, " ").split(/\.\s/)
            let finalDesc = ""
            let sentenceIdx = 0

            // Add full sentences until we exceed the guideline length
            while (sentenceIdx < sentences.length) {
              const sentence = sentences[sentenceIdx]
              if (!sentence) break

              const currentSentence = sentence.endsWith(".") ? sentence : sentence + "."
              const nextLength = finalDesc.length + currentSentence.length + (finalDesc ? 1 : 0)

              // Add the sentence if we're under the guideline length
              // or if this is the first sentence (always include at least one)
              if (nextLength <= opts.descriptionLength || sentenceIdx === 0) {
                finalDesc += (finalDesc ? " " : "") + currentSentence
                sentenceIdx++
              } else {
                break
              }
            }

            // truncate to max length if necessary
            file.data.description =
              finalDesc.length > opts.maxDescriptionLength
                ? finalDesc.slice(0, opts.maxDescriptionLength) + "..."
                : finalDesc
            file.data.text = text
          }
        },
      ]
    },
  }
}

declare module "vfile" {
  interface DataMap {
    description: string
    text: string
  }
}

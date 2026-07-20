import { QuartzComponent, QuartzComponentConstructor, QuartzComponentProps } from "./types"
import { classNames } from "../util/lang"

const CLIENT_ID = "1509817201622909079"
const INSTALL_URL = `https://discord.com/oauth2/authorize?client_id=${CLIENT_ID}&scope=bot+applications.commands`
const ACTIVITY_URL = `https://discord.com/activities/${CLIENT_ID}`

export default (() => {
  const DiscordBot: QuartzComponent = ({ displayClass }: QuartzComponentProps) => {
    return (
      <div class={classNames(displayClass, "discord-bot")}>
        <h3>Discord Bot</h3>
        <p>Look up reference entries and run DreamMaker code directly in Discord.</p>
        <div class="discord-bot-links">
          <a href={INSTALL_URL} class="discord-bot-button" target="_blank" rel="noreferrer">
            Add to Discord
          </a>
          <a href={ACTIVITY_URL} class="discord-bot-link" target="_blank" rel="noreferrer">
            Open Playground
          </a>
        </div>
      </div>
    )
  }

  DiscordBot.css = `
.discord-bot {
  background: var(--lightgray);
  border-radius: 8px;
  padding: 1rem;
}

.discord-bot h3 {
  margin: 0 0 0.5rem;
  font-size: 0.9rem;
}

.discord-bot p {
  margin: 0 0 0.75rem;
  font-size: 0.8rem;
  opacity: 0.85;
}

.discord-bot-links {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.discord-bot-button {
  display: block;
  text-align: center;
  padding: 0.5rem;
  border-radius: 6px;
  background: #5865f2;
  color: #fff !important;
  font-size: 0.85rem;
  font-weight: 500;
  text-decoration: none;
}

.discord-bot-button:hover {
  background: #4752c4;
}

.discord-bot-link {
  display: block;
  text-align: center;
  font-size: 0.8rem;
}
`

  return DiscordBot
}) satisfies QuartzComponentConstructor

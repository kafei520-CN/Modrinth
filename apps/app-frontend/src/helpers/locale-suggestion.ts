import { LOCALES, type PopupNotificationButton } from '@modrinth/ui'

import { translateForLocale } from '@/i18n.config'

const DISMISSED_SYSTEM_LOCALE_KEY = 'app.dismissed-system-locale'

const messages = {
	title: {
		id: 'app.locale-suggestion.title',
		fallback: 'Switch language',
	},
	body: {
		id: 'app.locale-suggestion.body',
		fallback: 'Your system language does not match the app language. Switch to {language}?',
	},
	ignore: {
		id: 'app.locale-suggestion.ignore',
		fallback: 'Ignore, don’t show again',
	},
	switchTo: {
		id: 'app.locale-suggestion.switch',
		fallback: "Switch to ''{language}''",
	},
}

type PopupNotifier = (notification: {
	contentType: 'standard'
	title: string
	text: string
	type: 'info'
	placement: 'bottom-right'
	autoCloseMs: null
	dismissible: boolean
	buttons: PopupNotificationButton[]
}) => void

export function resolveSupportedLocale(tag: string): string | null {
	const normalized = tag.trim().replaceAll('_', '-')
	if (!normalized) return null

	const exact = LOCALES.find(
		(locale) => locale.code.toLowerCase() === normalized.toLowerCase(),
	)
	if (exact) return exact.code

	const lower = normalized.toLowerCase()
	if (lower === 'zh' || lower.startsWith('zh-hans') || lower === 'zh-cn') return 'zh-CN'
	if (
		lower.startsWith('zh-hant') ||
		lower.startsWith('zh-tw') ||
		lower.startsWith('zh-hk') ||
		lower.startsWith('zh-mo')
	) {
		return 'zh-TW'
	}

	const language = lower.split('-')[0]
	const matches = LOCALES.filter((locale) => locale.code.toLowerCase().startsWith(`${language}-`))
	return matches[0]?.code ?? null
}

export function preferredSystemLocale(): string | null {
	const tags = [...(navigator.languages ?? [])]
	if (navigator.language) tags.push(navigator.language)
	const seen = new Set<string>()
	for (const tag of tags) {
		if (!tag || seen.has(tag)) continue
		seen.add(tag)
		const resolved = resolveSupportedLocale(tag)
		if (resolved) return resolved
	}
	return null
}

export async function maybeSuggestSystemLocale(options: {
	appLocale: string
	notify: PopupNotifier
	applyLocale: (locale: string) => Promise<void>
}): Promise<void> {
	const suggested = preferredSystemLocale()
	if (!suggested || suggested === options.appLocale) return
	if (localStorage.getItem(DISMISSED_SYSTEM_LOCALE_KEY) === suggested) return

	const language = LOCALES.find((locale) => locale.code === suggested)?.name ?? suggested
	const values = { language }
	const [title, text, ignore, switchTo] = await Promise.all([
		translateForLocale(suggested, messages.title.id, messages.title.fallback),
		translateForLocale(suggested, messages.body.id, messages.body.fallback, values),
		translateForLocale(suggested, messages.ignore.id, messages.ignore.fallback),
		translateForLocale(suggested, messages.switchTo.id, messages.switchTo.fallback, values),
	])

	options.notify({
		contentType: 'standard',
		title,
		text,
		type: 'info',
		placement: 'bottom-right',
		autoCloseMs: null,
		dismissible: true,
		buttons: [
			{
				label: ignore,
				color: 'standard',
				action: () => {
					localStorage.setItem(DISMISSED_SYSTEM_LOCALE_KEY, suggested)
				},
			},
			{
				label: switchTo,
				color: 'brand',
				action: () => options.applyLocale(suggested),
			},
		],
	})
}

<script setup lang="ts">
import { Settings2Icon } from '@modrinth/assets'
import {
	Button,
	defineMessages,
	injectNotificationManager,
	injectPageContext,
	Toggle,
	useVIntl,
} from '@modrinth/ui'
import { ref, watch } from 'vue'

import { useAppSettings } from '@/composables/use-app-settings.ts'
import { open_ads_consent_preferences } from '@/helpers/ads.js'
import { optInAnalytics, optOutAnalytics } from '@/helpers/analytics'
import { get, set } from '@/helpers/settings.ts'

const { formatMessage } = useVIntl()
const { handleError } = injectNotificationManager()
const { adConsentAvailable } = injectPageContext()
const appSettings = useAppSettings()
const settings = ref(await get())

const messages = defineMessages({
	adsConsentTitle: {
		id: 'app.ads-consent.title',
		defaultMessage: 'Your privacy and how ads support Modrinth',
	},
	adsConsentIntro: {
		id: 'app.settings.privacy.ads-consent.intro',
		defaultMessage:
			'Ads make Modrinth possible and fund creator payouts. Our partners may store or access cookies in the app to personalize ads and measure performance. You can opt out or manage your preferences below.',
	},
	adsConsentManage: {
		id: 'app.ads-consent.manage',
		defaultMessage: 'Manage preferences',
	},
	telemetryTitle: {
		id: 'app.settings.privacy.telemetry.title',
		defaultMessage: 'Telemetry',
	},
	telemetryDescription: {
		id: 'app.settings.privacy.telemetry.description',
		defaultMessage:
			'Modrinth collects anonymized analytics and usage data to improve our user experience and customize your experience. By disabling this option, you opt out and your data will no longer be collected.',
	},
	discordRichPresenceTitle: {
		id: 'app.settings.privacy.discord-rich-presence.title',
		defaultMessage: 'Discord Rich Presence',
	},
	discordRichPresenceDescription: {
		id: 'app.settings.privacy.discord-rich-presence.description',
		defaultMessage:
			'Show Modrinth App as your current activity on Discord. This does not affect Rich Presence added to instances by mods. Requires an app restart.',
	},
	newsTitle: {
		id: 'app.settings.privacy.news.title',
		defaultMessage: 'News',
	},
	newsDescription: {
		id: 'app.settings.privacy.news.description',
		defaultMessage: 'Show Modrinth news articles in the sidebar.',
	},
	adsTitle: {
		id: 'app.settings.privacy.ads.title',
		defaultMessage: 'Ads',
	},
	adsDescription: {
		id: 'app.settings.privacy.ads.description',
		defaultMessage:
			'Show advertisements, hosting promotions, and the Modrinth+ upgrade prompt in the sidebar.',
	},
})

async function manageAdsPreferences() {
	await open_ads_consent_preferences().catch(handleError)
}

watch(
	settings,
	async () => {
		if (settings.value.telemetry) {
			optInAnalytics()
		} else {
			optOutAnalytics()
		}

		appSettings.showAds = settings.value.show_ads
		appSettings.showNews = settings.value.show_news

		await set(settings.value)
	},
	{ deep: true },
)
</script>

<template>
	<div v-if="adConsentAvailable">
		<h2 class="m-0 text-lg font-semibold text-contrast">
			{{ formatMessage(messages.adsConsentTitle) }}
		</h2>
		<div class="mt-2 flex flex-col gap-2.5 items-start">
			<Button @click="manageAdsPreferences">
				<Settings2Icon aria-hidden="true" />
				{{ formatMessage(messages.adsConsentManage) }}
			</Button>
			<div>
				{{ formatMessage(messages.adsConsentIntro) }}
			</div>
		</div>
	</div>

	<div class="mt-8 first:mt-0 flex items-center justify-between gap-4">
		<div>
			<h2 class="m-0 text-lg font-semibold text-contrast">
				{{ formatMessage(messages.telemetryTitle) }}
			</h2>
			<p class="m-0 mt-1">
				{{ formatMessage(messages.telemetryDescription) }}
			</p>
		</div>
		<Toggle id="opt-out-analytics" v-model="settings.telemetry" />
	</div>

	<div class="mt-4 flex items-center justify-between gap-4">
		<div>
			<h2 class="m-0 text-lg font-semibold text-contrast">
				{{ formatMessage(messages.discordRichPresenceTitle) }}
			</h2>
			<p class="m-0 mt-1">
				{{ formatMessage(messages.discordRichPresenceDescription) }}
			</p>
		</div>
		<Toggle id="disable-discord-rpc" v-model="settings.discord_rpc" />
	</div>

	<div class="mt-4 flex items-center justify-between gap-4">
		<div>
			<h2 class="m-0 text-lg font-semibold text-contrast">
				{{ formatMessage(messages.newsTitle) }}
			</h2>
			<p class="m-0 mt-1">
				{{ formatMessage(messages.newsDescription) }}
			</p>
		</div>
		<Toggle id="show-news" v-model="settings.show_news" />
	</div>

	<div class="mt-4 flex items-center justify-between gap-4">
		<div>
			<h2 class="m-0 text-lg font-semibold text-contrast">
				{{ formatMessage(messages.adsTitle) }}
			</h2>
			<p class="m-0 mt-1">
				{{ formatMessage(messages.adsDescription) }}
			</p>
		</div>
		<Toggle id="show-ads" v-model="settings.show_ads" />
	</div>
</template>

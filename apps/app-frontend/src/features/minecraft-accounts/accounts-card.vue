<template>
	<div
		v-if="accounts.length === 0"
		class="flex flex-col gap-3 bg-button-bg border border-solid border-surface-5 rounded-xl p-3 mt-2"
	>
		<span>{{ formatMessage(messages.notSignedIn) }}</span>
		<Button type="colored" color="brand" :disabled="loginDisabled" @click="openAddAccount()">
			<LogInIcon v-if="!loginDisabled" />
			<SpinnerIcon v-else class="animate-spin" />
			{{ formatMessage(messages.signInToMinecraft) }}
		</Button>
	</div>
	<Accordion
		v-else
		class="w-full mt-2 bg-button-bg border border-solid border-surface-5 rounded-xl overflow-clip"
		button-class="button-base w-full bg-transparent px-3 py-2 border-0 cursor-pointer"
		:open-by-default="false"
	>
		<template #title>
			<div class="flex gap-2 w-full min-w-0">
				<Avatar
					size="36px"
					:src="
						selectedAccount
							? avatarUrl
							: 'https://launcher-files.modrinth.com/assets/steve_head.png'
					"
				/>
				<div class="flex flex-col items-start w-full min-w-0">
					<span class="truncate w-full text-left">{{
						selectedAccount ? selectedAccount.profile.name : formatMessage(messages.selectAccount)
					}}</span>
					<span class="text-secondary text-xs">{{ formatMessage(messages.minecraftAccount) }}</span>
				</div>
			</div>
		</template>
		<div class="bg-button-bg pt-1 pb-2 border-0 border-t border-solid border-surface-5">
			<template v-if="accounts.length > 0">
				<div v-for="account in accounts" :key="account.profile.id" class="flex gap-1 items-center">
					<button
						class="flex items-center flex-shrink flex-grow overflow-clip gap-2 p-2 border-0 bg-transparent cursor-pointer button-base min-w-0"
						@click="setAccount(account)"
					>
						<RadioButtonCheckedIcon
							v-if="selectedAccount && selectedAccount.profile.id === account.profile.id"
							class="w-5 h-5 text-brand shrink-0"
						/>
						<RadioButtonIcon v-else class="w-5 h-5 text-secondary shrink-0" />
						<Avatar :src="getAccountAvatarUrl(account)" size="24px" />
						<p
							class="m-0 truncate min-w-0"
							:class="
								selectedAccount && selectedAccount.profile.id === account.profile.id
									? 'text-contrast font-semibold'
									: 'text-primary'
							"
						>
							{{ account.profile.name }}
						</p>
					</button>
					<IconButton
						v-tooltip="formatMessage(messages.removeAccount)"
						type="quiet"
						color="red"
						:label="formatMessage(messages.removeAccount)"
						class="mr-2 !bg-button-bg !text-primary ![box-shadow:var(--shadow-button)] hover:!bg-red focus-visible:!bg-red hover:!text-[var(--color-accent-contrast)] focus-visible:!text-[var(--color-accent-contrast)]"
						@click="logout(account.profile.id)"
					>
						<TrashIcon />
					</IconButton>
				</div>
			</template>
			<div class="flex flex-col gap-2 px-2 pt-2">
				<Button
					v-if="accounts.length > 0"
					class="w-full !bg-button-bg !text-primary ![box-shadow:var(--shadow-button)]"
					:disabled="loginDisabled"
					@click="openAddAccount()"
				>
					<PlusIcon />
					{{ formatMessage(messages.addAccount) }}
				</Button>
			</div>
		</div>
	</Accordion>

	<NewModal ref="addAccountModal" :header="formatMessage(stepHeader)">
		<div v-if="accountStep === 'choose'" class="flex flex-col gap-3">
			<p class="m-0 text-sm text-secondary">{{ formatMessage(messages.addAccountHint) }}</p>
			<div class="grid grid-cols-1 gap-2">
				<button
					v-for="method in methods"
					:key="method.id"
					type="button"
					class="flex items-start gap-3 rounded-xl border border-solid p-3 text-left cursor-pointer bg-transparent"
					:class="
						selectedMethod === method.id
							? 'border-brand bg-brand-highlight'
							: 'border-surface-5 hover:bg-button-hover'
					"
					@click="selectedMethod = method.id"
				>
					<div
						class="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-button-bg text-brand"
					>
						<component :is="method.icon" class="h-5 w-5" />
					</div>
					<div class="min-w-0">
						<div class="font-semibold text-contrast">{{ formatMessage(method.title) }}</div>
						<div class="mt-0.5 text-xs text-secondary">{{ formatMessage(method.description) }}</div>
					</div>
				</button>
			</div>
		</div>

		<div v-else class="flex flex-col gap-3">
			<p class="m-0 text-sm text-secondary">
				{{ formatMessage(selectedMethodMessage.description) }}
			</p>
			<div v-if="selectedMethod === 'offline'" class="flex flex-col gap-2">
				<label class="text-sm font-medium text-contrast" for="offline-name">
					{{ formatMessage(messages.playerName) }}
				</label>
				<Input
					id="offline-name"
					v-model="offlineName"
					maxlength="16"
					:placeholder="formatMessage(messages.playerNamePlaceholder)"
					@keydown.enter="submitOffline"
				/>
			</div>
			<div v-else-if="selectedMethod === 'authlib'" class="flex flex-col gap-2">
				<label class="text-sm font-medium text-contrast" for="auth-server">
					{{ formatMessage(messages.authServer) }}
				</label>
				<Input
					id="auth-server"
					v-model="authServer"
					placeholder="https://littleskin.cn/api/yggdrasil"
				/>
				<label class="text-sm font-medium text-contrast" for="auth-username">
					{{ formatMessage(messages.username) }}
				</label>
				<Input id="auth-username" v-model="authUsername" autocomplete="username" />
				<label class="text-sm font-medium text-contrast" for="auth-password">
					{{ formatMessage(messages.password) }}
				</label>
				<Input
					id="auth-password"
					v-model="authPassword"
					type="password"
					autocomplete="current-password"
					@keydown.enter="submitAuthlib"
				/>
			</div>
		</div>

		<div class="mt-4 flex justify-end gap-2">
			<Button @click="accountStep === 'choose' ? addAccountModal?.hide() : backToMethods()">
				{{ formatMessage(accountStep === 'choose' ? messages.cancel : messages.back) }}
			</Button>
			<Button
				type="colored"
				color="brand"
				:disabled="loginDisabled || !selectedMethod"
				@click="accountStep === 'choose' ? goToLogin() : submitSelected()"
			>
				<SpinnerIcon v-if="loginDisabled" class="animate-spin" />
				{{ formatMessage(accountStep === 'choose' ? messages.next : messages.continue) }}
			</Button>
		</div>
	</NewModal>
</template>

<script setup lang="ts">
import {
	LogInIcon,
	PlusIcon,
	RadioButtonCheckedIcon,
	RadioButtonIcon,
	SpinnerIcon,
	TrashIcon,
	MicrosoftColorIcon,
	UserIcon,
	WorldIcon,
} from '@modrinth/assets'
import {
	Accordion,
	Avatar,
	Button,
	defineMessages,
	IconButton,
	injectNotificationManager,
	Input,
	NewModal,
	useVIntl,
} from '@modrinth/ui'
import type { Ref } from 'vue'
import { computed, onUnmounted, ref } from 'vue'

import { useAppEvent } from '@/composables/use-app-event'
import { handleSevereError } from '@/composables/use-error.js'
import { trackEvent } from '@/helpers/analytics'
import {
	get_default_user,
	login as login_flow,
	login_offline,
	login_yggdrasil,
	remove_user,
	set_default_user,
	users,
} from '@/helpers/auth'
import { getPlayerHeadUrl } from '@/helpers/rendering/player-head'
import type { Skin } from '@/helpers/skins'
import { get_available_skins } from '@/helpers/skins'

const { formatMessage } = useVIntl()
const { handleError } = injectNotificationManager()

const emit = defineEmits<{
	change: []
}>()

type MinecraftCredential = {
	profile: {
		id: string
		name: string
	}
}

const accounts: Ref<MinecraftCredential[]> = ref([])
const loginDisabled = ref(false)
const addAccountModal = ref<InstanceType<typeof NewModal> | null>(null)
const accountStep = ref<'choose' | 'login'>('choose')
const selectedMethod = ref<'offline' | 'microsoft' | 'authlib' | null>(null)
const offlineName = ref('')
const authServer = ref('')
const authUsername = ref('')
const authPassword = ref('')
const defaultUser = ref<string | undefined>()
const equippedSkin = ref<Skin | null>(null)
const equippedHeadUrl = ref<string>()
let headRequest = 0

async function updateHeadUrl(skin: Skin | null) {
	const request = ++headRequest
	if (equippedHeadUrl.value) URL.revokeObjectURL(equippedHeadUrl.value)
	equippedHeadUrl.value = undefined
	if (!skin) return
	const url = await getPlayerHeadUrl(skin)
	if (request !== headRequest) URL.revokeObjectURL(url)
	else equippedHeadUrl.value = url
}

onUnmounted(() => {
	headRequest++
	if (equippedHeadUrl.value) URL.revokeObjectURL(equippedHeadUrl.value)
})

async function refreshValues() {
	defaultUser.value = await get_default_user().catch(handleError)
	const userList = await users().catch(handleError)
	accounts.value = Array.isArray(userList) ? [...userList] : []
	accounts.value.sort((a, b) => (a.profile?.name ?? '').localeCompare(b.profile?.name ?? ''))

	try {
		const skins = await get_available_skins()
		equippedSkin.value = skins.find((skin) => skin.is_equipped) ?? null

		await updateHeadUrl(equippedSkin.value)
	} catch {
		equippedSkin.value = null
		void updateHeadUrl(null)
	}
}

async function setEquippedSkin(skin: Skin) {
	equippedSkin.value = skin

	try {
		await updateHeadUrl(skin)
	} catch (error) {
		console.warn('Failed to get head render for equipped skin:', error)
	}
}

function setLoginDisabled(value: boolean) {
	loginDisabled.value = value
}

defineExpose({
	refreshValues,
	setEquippedSkin,
	setLoginDisabled,
	login,
	loginDisabled,
})

await refreshValues()

const selectedAccount = computed(() =>
	accounts.value.find((account) => account.profile.id === defaultUser.value),
)

const avatarUrl = computed(() => {
	if (equippedSkin.value?.texture_key) {
		const cachedUrl = equippedHeadUrl.value
		if (cachedUrl) {
			return cachedUrl
		}
		return `https://mc-heads.net/avatar/${equippedSkin.value.texture_key}/128`
	}
	if (selectedAccount.value?.profile?.id) {
		return `https://mc-heads.net/avatar/${selectedAccount.value.profile.id}/128`
	}
	return 'https://launcher-files.modrinth.com/assets/steve_head.png'
})

function getAccountAvatarUrl(account: MinecraftCredential) {
	if (
		account.profile.id === selectedAccount.value?.profile?.id &&
		equippedSkin.value?.texture_key
	) {
		const cachedUrl = equippedHeadUrl.value
		if (cachedUrl) {
			return cachedUrl
		}
	}
	return `https://mc-heads.net/avatar/${account.profile.id}/128`
}

async function setAccount(account: MinecraftCredential) {
	defaultUser.value = account.profile.id
	await set_default_user(account.profile.id).catch(handleError)
	await refreshValues()
	emit('change')
}

function openAddAccount() {
	accountStep.value = 'choose'
	selectedMethod.value = null
	addAccountModal.value?.show()
}

function goToLogin() {
	if (!selectedMethod.value) return
	accountStep.value = 'login'
}

function backToMethods() {
	accountStep.value = 'choose'
}

async function login() {
	openAddAccount()
}

async function submitSelected() {
	if (selectedMethod.value === 'microsoft') {
		addAccountModal.value?.hide()
		await loginMicrosoft()
		return
	}
	if (selectedMethod.value === 'offline') {
		await submitOffline()
		return
	}
	await submitAuthlib()
}

async function loginMicrosoft() {
	loginDisabled.value = true
	const loggedIn = await login_flow().catch(handleSevereError)
	if (loggedIn) {
		await setAccount(loggedIn)
	}
	trackEvent('AccountLogIn')
	loginDisabled.value = false
}

async function submitOffline() {
	const name = offlineName.value.trim()
	if (!name) return
	loginDisabled.value = true
	const loggedIn = await login_offline(name).catch(handleSevereError)
	if (loggedIn) {
		offlineName.value = ''
		addAccountModal.value?.hide()
		await setAccount(loggedIn)
		trackEvent('AccountLogIn')
	}
	loginDisabled.value = false
}

async function submitAuthlib() {
	if (!authServer.value.trim() || !authUsername.value.trim() || !authPassword.value) return
	loginDisabled.value = true
	const loggedIn = await login_yggdrasil(
		authServer.value.trim(),
		authUsername.value.trim(),
		authPassword.value,
	).catch(handleSevereError)
	if (loggedIn) {
		authPassword.value = ''
		addAccountModal.value?.hide()
		await setAccount(loggedIn)
		trackEvent('AccountLogIn')
	}
	loginDisabled.value = false
}

async function logout(id: string) {
	await remove_user(id).catch(handleError)
	await refreshValues()
	if (!selectedAccount.value && accounts.value.length > 0) {
		await setAccount(accounts.value[0])
	} else {
		emit('change')
	}
	trackEvent('AccountLogOut')
}

useAppEvent('process', async (e) => {
	if (e.event === 'launched') {
		await refreshValues()
	}
})

const messages = defineMessages({
	notSignedIn: {
		id: 'minecraft-account.not-signed-in',
		defaultMessage: 'Not signed in',
	},
	addAccount: {
		id: 'minecraft-account.add-account',
		defaultMessage: 'Add account',
	},
	removeAccount: {
		id: 'minecraft-account.remove-account',
		defaultMessage: 'Remove account',
	},
	selectAccount: {
		id: 'minecraft-account.select-account',
		defaultMessage: 'Select account',
	},
	minecraftAccount: {
		id: 'minecraft-account.label',
		defaultMessage: 'Minecraft account',
	},
	signInToMinecraft: {
		id: 'minecraft-account.sign-in',
		defaultMessage: 'Sign in to Minecraft',
	},
	addAccountTitle: {
		id: 'minecraft-account.add-account.title',
		defaultMessage: 'Add account',
	},
	addAccountHint: {
		id: 'minecraft-account.add-account.hint',
		defaultMessage: 'Choose how you want to sign in.',
	},
	offlineTitle: {
		id: 'minecraft-account.method.offline.title',
		defaultMessage: 'Offline',
	},
	offlineDescription: {
		id: 'minecraft-account.method.offline.description',
		defaultMessage: 'Play with a username. No Microsoft account required.',
	},
	microsoftTitle: {
		id: 'minecraft-account.method.microsoft.title',
		defaultMessage: 'Microsoft',
	},
	microsoftDescription: {
		id: 'minecraft-account.method.microsoft.description',
		defaultMessage: 'Sign in with a Microsoft account that owns Minecraft.',
	},
	authlibTitle: {
		id: 'minecraft-account.method.authlib.title',
		defaultMessage: 'Third-party auth',
	},
	authlibDescription: {
		id: 'minecraft-account.method.authlib.description',
		defaultMessage: 'Sign in to a Yggdrasil server such as LittleSkin.',
	},
	playerName: {
		id: 'minecraft-account.player-name',
		defaultMessage: 'Player name',
	},
	playerNamePlaceholder: {
		id: 'minecraft-account.player-name.placeholder',
		defaultMessage: 'Steve',
	},
	authServer: {
		id: 'minecraft-account.auth-server',
		defaultMessage: 'Auth server',
	},
	username: {
		id: 'minecraft-account.username',
		defaultMessage: 'Username',
	},
	password: {
		id: 'minecraft-account.password',
		defaultMessage: 'Password',
	},
	cancel: {
		id: 'minecraft-account.cancel',
		defaultMessage: 'Cancel',
	},
	continue: {
		id: 'minecraft-account.continue',
		defaultMessage: 'Continue',
	},
	next: {
		id: 'minecraft-account.next',
		defaultMessage: 'Next',
	},
	back: {
		id: 'minecraft-account.back',
		defaultMessage: 'Back',
	},
})

const stepHeader = computed(() =>
	accountStep.value === 'choose'
		? messages.addAccountTitle
		: (methods.find((method) => method.id === selectedMethod.value)?.title ??
			messages.addAccountTitle),
)
const selectedMethodMessage = computed(
	() => methods.find((method) => method.id === selectedMethod.value) ?? methods[0],
)

const methods = [
	{
		id: 'offline' as const,
		icon: UserIcon,
		title: messages.offlineTitle,
		description: messages.offlineDescription,
	},
	{
		id: 'microsoft' as const,
		icon: MicrosoftColorIcon,
		title: messages.microsoftTitle,
		description: messages.microsoftDescription,
	},
	{
		id: 'authlib' as const,
		icon: WorldIcon,
		title: messages.authlibTitle,
		description: messages.authlibDescription,
	},
]
</script>

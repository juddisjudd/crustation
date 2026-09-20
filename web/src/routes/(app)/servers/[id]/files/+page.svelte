<script lang="ts">
	import DownloadIcon from '@lucide/svelte/icons/download';
	import EllipsisIcon from '@lucide/svelte/icons/ellipsis';
	import FileIcon from '@lucide/svelte/icons/file';
	import FilePlusIcon from '@lucide/svelte/icons/file-plus';
	import FolderIcon from '@lucide/svelte/icons/folder';
	import FolderPlusIcon from '@lucide/svelte/icons/folder-plus';
	import PackageOpenIcon from '@lucide/svelte/icons/package-open';
	import PencilIcon from '@lucide/svelte/icons/pencil';
	import Trash2Icon from '@lucide/svelte/icons/trash-2';
	import UploadIcon from '@lucide/svelte/icons/upload';
	import { toast } from 'svelte-sonner';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import { Spinner } from '$lib/components/ui/spinner/index.js';
	import { Textarea } from '$lib/components/ui/textarea/index.js';
	import * as Alert from '$lib/components/ui/alert/index.js';
	import * as Breadcrumb from '$lib/components/ui/breadcrumb/index.js';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import * as Empty from '$lib/components/ui/empty/index.js';
	import * as Table from '$lib/components/ui/table/index.js';
	import { confirm } from '$lib/components/confirm/confirm.svelte';
	import { errorMessage } from '$lib/api/servers';
	import {
		createEntry,
		deleteEntries,
		downloadUrl,
		extractArchive,
		joinPath,
		listFiles,
		readFile,
		renameEntry,
		saveFile,
		uploadFile,
		type FileContent,
		type FileEntry,
		type Listing
	} from '$lib/api/files';
	import { bytes, dateTime } from '$lib/format';
	import { t } from '$lib/i18n/index.svelte';

	let { data } = $props();

	const server = $derived(data.server);

	let path = $state('');
	let listing = $state.raw<Listing | null>(null);
	let loadFailed = $state(false);
	let busy = $state(false);

	let open = $state.raw<FileContent | null>(null);
	let draft = $state('');
	let saving = $state(false);

	let uploadInput = $state<HTMLInputElement | null>(null);

	const crumbs = $derived(path ? path.split('/') : []);
	const dirty = $derived(!!open && draft !== open.content);

	async function refresh() {
		loadFailed = false;
		try {
			listing = await listFiles(server.id, path);
		} catch {
			loadFailed = true;
		}
	}

	$effect(() => {
		// Re-reads on a new folder, and when the page moves to another server.
		void server.id;
		void path;
		refresh();
	});

	function go(to: string) {
		if (dirty) return;
		open = null;
		path = to;
	}

	async function enter(entry: FileEntry) {
		if (entry.kind === 'directory') {
			go(joinPath(path, entry.name));
			return;
		}
		try {
			const file = await readFile(server.id, joinPath(path, entry.name));
			open = file;
			draft = file.content;
		} catch (err) {
			toast.error(t('files.editor.tooBig'), { description: errorMessage(err) });
		}
	}

	async function act(what: () => Promise<unknown>, done: string) {
		busy = true;
		try {
			await what();
			toast.success(done);
			await refresh();
		} catch (err) {
			toast.error(t('files.failed'), { description: errorMessage(err) });
		} finally {
			busy = false;
		}
	}

	function make(kind: 'file' | 'directory') {
		const label = kind === 'file' ? t('files.prompt.newFile') : t('files.prompt.newFolder');
		const name = window.prompt(label)?.trim();
		if (!name) return;
		act(
			() => createEntry(server.id, joinPath(path, name), kind),
			t('files.done.created', { name })
		);
	}

	function rename(entry: FileEntry) {
		const name = window.prompt(t('files.prompt.rename', { name: entry.name }), entry.name)?.trim();
		if (!name || name === entry.name) return;
		act(
			() => renameEntry(server.id, joinPath(path, entry.name), name),
			t('files.done.renamed', { name })
		);
	}

	async function remove(entry: FileEntry) {
		const ok = await confirm({
			title: t('files.confirm.deleteTitle', { name: entry.name }),
			description:
				entry.kind === 'directory'
					? t('files.confirm.deleteFolder')
					: t('files.confirm.deleteBody'),
			confirmLabel: t('files.actions.delete'),
			destructive: true
		});
		if (!ok) return;
		if (open?.path === joinPath(path, entry.name)) open = null;
		act(
			() => deleteEntries(server.id, [joinPath(path, entry.name)]),
			t('files.done.deleted', { name: entry.name })
		);
	}

	async function unpack(entry: FileEntry) {
		const ok = await confirm({
			title: t('files.confirm.unpackTitle', { name: entry.name }),
			description: t('files.confirm.unpackBody'),
			confirmLabel: t('files.actions.unpack')
		});
		if (!ok) return;
		act(
			() => extractArchive(server.id, joinPath(path, entry.name)),
			t('files.done.unpacked', { name: entry.name })
		);
	}

	async function onUpload(event: Event & { currentTarget: HTMLInputElement }) {
		const file = event.currentTarget.files?.[0];
		event.currentTarget.value = '';
		if (!file) return;
		await act(
			() => uploadFile(server.id, joinPath(path, file.name), file),
			t('files.done.uploaded', { name: file.name })
		);
	}

	async function save() {
		if (!open) return;
		saving = true;
		try {
			const result = await saveFile(server.id, open.path, draft, open.modified);
			open = { ...open, content: draft, ...result };
			toast.success(t('files.editor.saved', { name: open.path }));
			await refresh();
		} catch (err) {
			toast.error(t('files.failed'), { description: errorMessage(err) });
		} finally {
			saving = false;
		}
	}
</script>

<svelte:head><title>{t('nav.tabs.files')} · {server.name} · Crustation</title></svelte:head>

<div class="mx-auto w-full max-w-6xl px-4 py-6 md:px-8">
	<div class="flex flex-wrap items-center justify-between gap-3">
		<Breadcrumb.Root>
			<Breadcrumb.List>
				<Breadcrumb.Item>
					{#if path}
						<Breadcrumb.Link
							class="cursor-pointer"
							onclick={() => go('')}
							role="button"
							tabindex={0}
						>
							{t('files.root')}
						</Breadcrumb.Link>
					{:else}
						<Breadcrumb.Page>{t('files.root')}</Breadcrumb.Page>
					{/if}
				</Breadcrumb.Item>
				{#each crumbs as part, index (index)}
					{@const to = crumbs.slice(0, index + 1).join('/')}
					<Breadcrumb.Separator />
					<Breadcrumb.Item>
						{#if index < crumbs.length - 1}
							<Breadcrumb.Link
								class="cursor-pointer"
								onclick={() => go(to)}
								role="button"
								tabindex={0}
							>
								{part}
							</Breadcrumb.Link>
						{:else}
							<Breadcrumb.Page>{part}</Breadcrumb.Page>
						{/if}
					</Breadcrumb.Item>
				{/each}
			</Breadcrumb.List>
		</Breadcrumb.Root>

		<div class="flex items-center gap-2">
			{#if busy}<Spinner class="size-4" />{/if}
			<Button variant="outline" size="sm" onclick={() => make('directory')} disabled={busy}>
				<FolderPlusIcon />
				{t('files.actions.newFolder')}
			</Button>
			<Button variant="outline" size="sm" onclick={() => make('file')} disabled={busy}>
				<FilePlusIcon />
				{t('files.actions.newFile')}
			</Button>
			<Button variant="outline" size="sm" onclick={() => uploadInput?.click()} disabled={busy}>
				<UploadIcon />
				{t('files.actions.upload')}
			</Button>
			<input bind:this={uploadInput} type="file" class="hidden" onchange={onUpload} />
		</div>
	</div>

	{#if loadFailed}
		<Alert.Root variant="destructive" class="mt-6">
			<Alert.Description>{t('files.loadFailed')}</Alert.Description>
		</Alert.Root>
	{:else if !listing}
		<Skeleton class="mt-6 h-64 rounded-lg" />
	{:else if listing.entries.length === 0}
		<Empty.Root class="mt-6 rounded-lg border border-dashed py-16">
			<Empty.Header>
				<Empty.Media variant="icon"><FolderIcon /></Empty.Media>
				<Empty.Title>{t('files.empty')}</Empty.Title>
			</Empty.Header>
		</Empty.Root>
	{:else}
		<div class="mt-6 overflow-hidden rounded-lg border">
			<Table.Root>
				<Table.Header>
					<Table.Row class="bg-muted/50 hover:bg-muted/50">
						<Table.Head>{t('files.columns.name')}</Table.Head>
						<Table.Head class="hidden w-28 md:table-cell">{t('files.columns.size')}</Table.Head>
						<Table.Head class="hidden w-48 md:table-cell">
							{t('files.columns.modified')}
						</Table.Head>
						<Table.Head class="w-12"></Table.Head>
					</Table.Row>
				</Table.Header>
				<Table.Body>
					{#each listing.entries as entry (entry.name)}
						<Table.Row>
							<Table.Cell>
								<button
									class="flex items-center gap-2 text-left hover:underline"
									onclick={() => enter(entry)}
								>
									{#if entry.kind === 'directory'}
										<FolderIcon class="size-4 shrink-0 text-muted-foreground" />
									{:else}
										<FileIcon class="size-4 shrink-0 text-muted-foreground" />
									{/if}
									<span class="truncate">{entry.name}</span>
								</button>
							</Table.Cell>
							<Table.Cell class="hidden text-sm text-muted-foreground tabular-nums md:table-cell">
								{entry.kind === 'directory' ? '—' : bytes(entry.size)}
							</Table.Cell>
							<Table.Cell class="hidden text-sm text-muted-foreground md:table-cell">
								{dateTime(entry.modified)}
							</Table.Cell>
							<Table.Cell>
								<DropdownMenu.Root>
									<DropdownMenu.Trigger>
										{#snippet child({ props })}
											<Button
												{...props}
												variant="ghost"
												size="icon-sm"
												aria-label={t('files.actions.more', { name: entry.name })}
											>
												<EllipsisIcon />
											</Button>
										{/snippet}
									</DropdownMenu.Trigger>
									<DropdownMenu.Content align="end">
										<DropdownMenu.Item>
											{#snippet child({ props })}
												<a
													{...props}
													href={downloadUrl(server.id, joinPath(path, entry.name))}
													download
												>
													<DownloadIcon />
													{t('files.actions.download')}
												</a>
											{/snippet}
										</DropdownMenu.Item>
										<DropdownMenu.Item onSelect={() => rename(entry)}>
											<PencilIcon />
											{t('files.actions.rename')}
										</DropdownMenu.Item>
										{#if entry.name.toLowerCase().endsWith('.zip')}
											<DropdownMenu.Item onSelect={() => unpack(entry)}>
												<PackageOpenIcon />
												{t('files.actions.unpack')}
											</DropdownMenu.Item>
										{/if}
										<DropdownMenu.Separator />
										<DropdownMenu.Item variant="destructive" onSelect={() => remove(entry)}>
											<Trash2Icon />
											{t('files.actions.delete')}
										</DropdownMenu.Item>
									</DropdownMenu.Content>
								</DropdownMenu.Root>
							</Table.Cell>
						</Table.Row>
					{/each}
				</Table.Body>
			</Table.Root>
		</div>
	{/if}

	{#if open}
		<section class="mt-6 overflow-hidden rounded-lg border bg-card">
			<div class="flex flex-wrap items-center justify-between gap-2 border-b px-4 py-2.5">
				<span class="truncate font-mono text-xs">{open.path}</span>
				<div class="flex items-center gap-2">
					{#if dirty}
						<span class="text-xs text-warning">{t('files.editor.unsaved')}</span>
					{/if}
					<Button variant="outline" size="sm" onclick={() => (open = null)}>
						{t('files.editor.close')}
					</Button>
					<Button size="sm" onclick={save} disabled={saving || !dirty}>
						{#if saving}<Spinner class="size-4" />{/if}
						{saving ? t('files.editor.saving') : t('files.editor.save')}
					</Button>
				</div>
			</div>
			<Textarea
				bind:value={draft}
				spellcheck={false}
				class="min-h-[28rem] resize-y rounded-none border-0 font-mono text-xs focus-visible:ring-0"
			/>
		</section>
	{/if}
</div>

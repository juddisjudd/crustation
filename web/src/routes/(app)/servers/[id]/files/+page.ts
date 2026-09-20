export async function load({ parent }) {
	const { crumbs } = await parent();
	return { crumbs: [...crumbs, { labelKey: 'nav.tabs.files' as const }] };
}

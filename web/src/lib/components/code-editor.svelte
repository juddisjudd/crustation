<script lang="ts">
	import { EditorState, type Extension } from '@codemirror/state';
	import {
		EditorView,
		drawSelection,
		highlightActiveLine,
		highlightActiveLineGutter,
		highlightSpecialChars,
		keymap,
		lineNumbers
	} from '@codemirror/view';
	import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
	import { highlightSelectionMatches, searchKeymap } from '@codemirror/search';
	import {
		HighlightStyle,
		StreamLanguage,
		bracketMatching,
		indentUnit,
		syntaxHighlighting
	} from '@codemirror/language';
	import { tags } from '@lezer/highlight';

	let {
		value = $bindable(''),
		filename = '',
		readonly = false,
		class: className = ''
	}: {
		value?: string;
		/** Only the extension is read, to pick the language. */
		filename?: string;
		readonly?: boolean;
		class?: string;
	} = $props();

	let host = $state<HTMLDivElement | null>(null);
	let view: EditorView | null = null;

	// Colours come from the block below, so light and dark follow the app.
	const highlight = HighlightStyle.define([
		{ tag: tags.comment, color: 'var(--cm-comment)', fontStyle: 'italic' },
		{ tag: [tags.string, tags.special(tags.string)], color: 'var(--cm-string)' },
		{ tag: [tags.number, tags.bool, tags.null], color: 'var(--cm-number)' },
		{ tag: [tags.keyword, tags.modifier], color: 'var(--cm-keyword)' },
		{ tag: [tags.propertyName, tags.attributeName], color: 'var(--cm-property)' },
		{ tag: [tags.typeName, tags.className, tags.tagName], color: 'var(--cm-type)' },
		{ tag: [tags.function(tags.variableName), tags.labelName], color: 'var(--cm-function)' },
		{ tag: [tags.operator, tags.punctuation, tags.separator], color: 'var(--cm-punctuation)' },
		{ tag: tags.invalid, color: 'var(--destructive)' }
	]);

	const look = EditorView.theme({
		'&': { backgroundColor: 'transparent', color: 'var(--foreground)' },
		'&.cm-focused': { outline: 'none' },
		'.cm-scroller': {
			fontFamily: 'var(--font-mono)',
			fontSize: '12px',
			lineHeight: '1.6'
		},
		'.cm-content': { padding: '12px 0' },
		'.cm-gutters': {
			backgroundColor: 'transparent',
			color: 'var(--muted-foreground)',
			border: 'none'
		},
		'.cm-activeLine, .cm-activeLineGutter': { backgroundColor: 'var(--muted)' },
		'.cm-selectionBackground, ::selection': { backgroundColor: 'var(--accent)' },
		'.cm-cursor': { borderLeftColor: 'var(--foreground)' },
		'.cm-selectionMatch': { backgroundColor: 'var(--accent)' }
	});

	/** Loaded on demand, so nine grammars do not ride along in the first bundle. */
	async function languageFor(name: string): Promise<Extension | null> {
		const extension = name.split('.').pop()?.toLowerCase() ?? '';
		switch (extension) {
			case 'json':
			case 'mcmeta':
				return (await import('@codemirror/lang-json')).json();
			case 'yml':
			case 'yaml':
				return (await import('@codemirror/lang-yaml')).yaml();
			case 'xml':
				return (await import('@codemirror/lang-xml')).xml();
			case 'html':
				return (await import('@codemirror/lang-html')).html();
			case 'css':
				return (await import('@codemirror/lang-css')).css();
			case 'js':
			case 'mjs':
				return (await import('@codemirror/lang-javascript')).javascript();
			case 'java':
				return (await import('@codemirror/lang-java')).java();
			case 'py':
				return (await import('@codemirror/lang-python')).python();
			case 'md':
				return (await import('@codemirror/lang-markdown')).markdown();
			case 'properties':
			case 'conf':
			case 'cfg':
			case 'ini':
				return StreamLanguage.define(
					(await import('@codemirror/legacy-modes/mode/properties')).properties
				);
			case 'toml':
				return StreamLanguage.define((await import('@codemirror/legacy-modes/mode/toml')).toml);
			case 'sh':
			case 'bash':
				return StreamLanguage.define((await import('@codemirror/legacy-modes/mode/shell')).shell);
			default:
				return null;
		}
	}

	function extensions(language: Extension | null): Extension[] {
		return [
			lineNumbers(),
			highlightActiveLineGutter(),
			highlightActiveLine(),
			highlightSpecialChars(),
			highlightSelectionMatches(),
			drawSelection(),
			bracketMatching(),
			history(),
			indentUnit.of('  '),
			keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap, indentWithTab]),
			syntaxHighlighting(highlight),
			look,
			EditorView.lineWrapping,
			EditorState.readOnly.of(readonly),
			EditorView.updateListener.of((update) => {
				if (update.docChanged) value = update.state.doc.toString();
			}),
			...(language ? [language] : [])
		];
	}

	$effect(() => {
		const element = host;
		if (!element) return;
		// Rebuilt when the file changes, because the language comes from its name.
		const wanted = filename;
		let alive = true;

		languageFor(wanted).then((language) => {
			if (!alive || !element) return;
			view?.destroy();
			view = new EditorView({
				parent: element,
				state: EditorState.create({ doc: value, extensions: extensions(language) })
			});
		});

		return () => {
			alive = false;
			view?.destroy();
			view = null;
		};
	});

	$effect(() => {
		// A save or a reload replaces the text under the editor.
		const next = value;
		if (view && view.state.doc.toString() !== next) {
			view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: next } });
		}
	});
</script>

<div bind:this={host} class="cm-host {className}"></div>

<style>
	.cm-host {
		--cm-comment: #6b7280;
		--cm-string: #0a7d55;
		--cm-number: #b45309;
		--cm-keyword: #7c3aed;
		--cm-property: #1d4ed8;
		--cm-type: #0e7490;
		--cm-function: #b91c1c;
		--cm-punctuation: #6b7280;
		overflow: auto;
	}

	/* Fill the box it is given, so a short file still gets the whole editor
	   rather than a band of text above dead space. */
	.cm-host :global(.cm-editor) {
		height: 100%;
	}

	:global(.dark) .cm-host {
		--cm-comment: #8b949e;
		--cm-string: #7fd1b9;
		--cm-number: #e3b341;
		--cm-keyword: #d2a8ff;
		--cm-property: #79c0ff;
		--cm-type: #56d4dd;
		--cm-function: #ffa198;
		--cm-punctuation: #8b949e;
	}
</style>

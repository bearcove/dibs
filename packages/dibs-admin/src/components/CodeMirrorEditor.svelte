<script lang="ts">
    import { onMount, onDestroy } from "svelte";
    import { EditorView, keymap, placeholder as placeholderExt } from "@codemirror/view";
    import { EditorState, type Extension } from "@codemirror/state";
    import { markdown } from "@codemirror/lang-markdown";
    import { json } from "@codemirror/lang-json";
    import { html } from "@codemirror/lang-html";
    import { css } from "@codemirror/lang-css";
    import { javascript } from "@codemirror/lang-javascript";
    import { vim } from "@replit/codemirror-vim";
    import { oneDark } from "@codemirror/theme-one-dark";
    import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
    import { syntaxHighlighting, defaultHighlightStyle } from "@codemirror/language";

    interface Props {
        value: string;
        lang?: string | null;
        disabled?: boolean;
        placeholder?: string;
        onchange?: (value: string) => void;
        class?: string;
    }

    let {
        value,
        lang = null,
        disabled = false,
        placeholder = "",
        onchange,
        class: className = "",
    }: Props = $props();

    let container: HTMLDivElement;
    let view: EditorView | null = null;

    function getLanguageExtension(lang: string | null): Extension[] {
        switch (lang?.toLowerCase()) {
            case "markdown":
            case "md":
                return [markdown()];
            case "json":
                return [json()];
            case "html":
                return [html()];
            case "css":
                return [css()];
            case "javascript":
            case "js":
                return [javascript()];
            case "typescript":
            case "ts":
                return [javascript({ typescript: true })];
            default:
                return [];
        }
    }

    function createEditor() {
        if (!container) return;

        const extensions: Extension[] = [
            vim(),
            history(),
            keymap.of([...defaultKeymap, ...historyKeymap]),
            syntaxHighlighting(defaultHighlightStyle),
            oneDark,
            EditorView.lineWrapping,
            EditorView.updateListener.of((update) => {
                if (update.docChanged && onchange) {
                    onchange(update.state.doc.toString());
                }
            }),
            ...getLanguageExtension(lang),
        ];

        if (placeholder) {
            extensions.push(placeholderExt(placeholder));
        }

        if (disabled) {
            extensions.push(EditorState.readOnly.of(true));
        }

        const state = EditorState.create({
            doc: value,
            extensions,
        });

        view = new EditorView({
            state,
            parent: container,
        });
    }

    function destroyEditor() {
        if (view) {
            view.destroy();
            view = null;
        }
    }

    onMount(() => {
        createEditor();
    });

    onDestroy(() => {
        destroyEditor();
    });

    // Update content when value prop changes externally
    $effect(() => {
        if (view && value !== view.state.doc.toString()) {
            view.dispatch({
                changes: {
                    from: 0,
                    to: view.state.doc.length,
                    insert: value,
                },
            });
        }
    });
</script>

<div
    bind:this={container}
    class="codemirror-wrapper border border-input rounded-md overflow-hidden {className}"
    class:opacity-50={disabled}
></div>

<style>
    .codemirror-wrapper :global(.cm-editor) {
        min-height: 150px;
        max-height: 400px;
        font-size: 14px;
    }

    .codemirror-wrapper :global(.cm-scroller) {
        overflow: auto;
    }

    .codemirror-wrapper :global(.cm-focused) {
        outline: none;
    }
</style>

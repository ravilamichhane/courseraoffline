import katex from "katex";
import renderMathInElement from "katex/contrib/auto-render";

function normalizeMathTextNodes(container: HTMLElement) {
	const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT, {
		acceptNode(node) {
			const parent = node.parentElement;
			if (
				parent &&
				["script", "noscript", "style", "textarea", "pre", "code"].includes(
					parent.tagName.toLowerCase(),
				)
			) {
				return NodeFilter.FILTER_REJECT;
			}
			return NodeFilter.FILTER_ACCEPT;
		},
	});

	const textNodes: Text[] = [];
	let current = walker.nextNode();
	while (current) {
		textNodes.push(current as Text);
		current = walker.nextNode();
	}

	for (const node of textNodes) {
		if (node.nodeValue?.includes("$$")) {
			node.nodeValue = node.nodeValue.replace(
				/\$\$([\s\S]+?)\$\$([.,;!?])(?=\s*(?:<|$))/g,
				(_, math, punct) => `$$${math}${punct}$$`,
			);
		}
	}
}

/**
 * Parses and renders LaTeX mathematics inside an HTML container.
 * Supports:
 * - Block math: `$$...$$`, `\\[...\\]`
 * - Inline math: `$...$`, `\\(...\\)`
 * - Coursera CML elements with `data-has-math="true"`
 */
export function renderMath(container: HTMLElement | null): void {
	if (!container) return;

	try {
		// 1. Move trailing punctuation into display formulas so periods/commas don't wrap to new lines
		normalizeMathTextNodes(container);

		// 2. Render math expressions using KaTeX auto-render
		renderMathInElement(container, {
			delimiters: [
				{ left: "$$", right: "$$", display: true },
				{ left: "\\[", right: "\\]", display: true },
				{ left: "\\(", right: "\\)", display: false },
				{ left: "$", right: "$", display: false },
			],
			ignoredTags: ["script", "noscript", "style", "textarea", "pre", "code"],
			throwOnError: false,
			errorColor: "#ef4444",
		});

		// 3. Handle any Coursera CML tags marked with data-has-math="true" that were not caught
		const mathNodes = container.querySelectorAll<HTMLElement>(
			'[data-has-math="true"]',
		);
		for (const node of mathNodes) {
			if (node.querySelector(".katex")) continue;
			const text = node.textContent?.trim();
			if (!text) continue;

			const cleanText = text.replace(/^\${1,2}([\s\S]*?)\${1,2}$/, "$1").trim();
			if (!cleanText) continue;

			try {
				katex.render(cleanText, node, {
					displayMode: node.tagName === "P" || node.tagName === "DIV",
					throwOnError: false,
					errorColor: "#ef4444",
				});
			} catch {
				// Keep original content if parsing fails
			}
		}
	} catch (error) {
		console.error("Failed to render math expressions:", error);
	}
}

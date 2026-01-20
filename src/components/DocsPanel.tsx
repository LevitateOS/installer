import { Box, Text, useInput } from "ink"
import { useState } from "react"
import {
	docsNav,
	contentBySlug,
	type DocsContent,
	type ContentBlock,
	type RichText,
} from "@levitate/docs-content"

/** Convert RichText to plain string for terminal display */
function toPlainText(content: string | RichText): string {
	if (typeof content === "string") return content
	return content
		.map((node) => {
			if (typeof node === "string") return node
			return node.text
		})
		.join("")
}

export function DocsPanel() {
	const allItems = docsNav.flatMap((section) => section.items)
	const [selectedIdx, setSelectedIdx] = useState(0)
	const [scrollOffset, setScrollOffset] = useState(0)

	const currentItem = allItems[selectedIdx]
	// Extract slug from href (e.g., "/docs/installation" -> "installation")
	const slug = currentItem?.href.replace("/docs/", "") || ""
	const content = contentBySlug[slug] || null

	useInput((input, key) => {
		// Navigate docs list
		if (key.upArrow) {
			setSelectedIdx((prev) => Math.max(0, prev - 1))
			setScrollOffset(0)
		}
		if (key.downArrow) {
			setSelectedIdx((prev) => Math.min(allItems.length - 1, prev + 1))
			setScrollOffset(0)
		}
		// Scroll content
		if (input === "j") {
			setScrollOffset((prev) => prev + 1)
		}
		if (input === "k") {
			setScrollOffset((prev) => Math.max(0, prev - 1))
		}
	})

	return (
		<Box flexDirection="row" flexGrow={1}>
			{/* Navigation sidebar */}
			<Box
				flexDirection="column"
				width={24}
				borderStyle="single"
				borderRight
				borderTop={false}
				borderBottom={false}
				borderLeft={false}
				paddingX={1}
			>
				{docsNav.map((section) => (
					<Box key={section.title} flexDirection="column" marginBottom={1}>
						<Text bold color="cyan">
							{section.title}
						</Text>
						{section.items.map((item) => {
							const idx = allItems.findIndex((i) => i.href === item.href)
							const isSelected = idx === selectedIdx
							return (
								<Text
									key={item.href}
									color={isSelected ? "green" : undefined}
									inverse={isSelected}
								>
									{isSelected ? "▸ " : "  "}
									{item.title}
								</Text>
							)
						})}
					</Box>
				))}
				<Text dimColor>↑↓ select | j/k scroll</Text>
			</Box>

			{/* Content area */}
			<Box flexDirection="column" flexGrow={1} paddingX={1} paddingY={0}>
				{content ? (
					<ContentRenderer content={content} scrollOffset={scrollOffset} />
				) : (
					<Text dimColor>Select a document</Text>
				)}
			</Box>
		</Box>
	)
}

interface ContentRendererProps {
	content: DocsContent
	scrollOffset: number
}

function ContentRenderer({ content, scrollOffset }: ContentRendererProps) {
	// Flatten all content into renderable lines
	const lines: React.ReactNode[] = []

	lines.push(
		<Text key="title" bold color="cyan">
			{content.title}
		</Text>
	)
	lines.push(<Text key="spacer1"> </Text>)

	if (content.intro) {
		lines.push(
			<Text key="intro" wrap="wrap" dimColor>
				{toPlainText(content.intro)}
			</Text>
		)
		lines.push(<Text key="spacer2"> </Text>)
	}

	content.sections.forEach((section, sIdx) => {
		const level = section.level ?? 2
		lines.push(
			<Text key={`section-${sIdx}`} bold color={level === 2 ? "white" : "gray"}>
				{level === 3 ? "  " : ""}
				{section.title}
			</Text>
		)

		section.content.forEach((block, bIdx) => {
			const key = `block-${sIdx}-${bIdx}`
			lines.push(renderBlock(block, key))
		})

		lines.push(<Text key={`spacer-${sIdx}`}> </Text>)
	})

	// Apply scroll offset
	const visibleLines = lines.slice(scrollOffset)

	return (
		<Box flexDirection="column">
			{visibleLines.map((line, i) => (
				<Box key={i}>{line}</Box>
			))}
		</Box>
	)
}

function renderBlock(block: ContentBlock, key: string): React.ReactNode {
	switch (block.type) {
		case "text":
			return (
				<Text key={key} wrap="wrap">
					{toPlainText(block.content)}
				</Text>
			)

		case "code":
			return (
				<Box key={key} flexDirection="column" marginY={0}>
					{block.filename && <Text dimColor>{block.filename}</Text>}
					<Box paddingX={1}>
						<Text color="yellow">{block.content}</Text>
					</Box>
				</Box>
			)

		case "list":
			return (
				<Box key={key} flexDirection="column" marginLeft={2}>
					{block.items.map((item, i) => (
						<Text key={i}>
							{"• "}
							{typeof item === "string"
								? item
								: Array.isArray(item)
									? toPlainText(item)
									: toPlainText(item.text)}
						</Text>
					))}
				</Box>
			)

		case "table":
			return (
				<Box key={key} flexDirection="column" marginY={0}>
					<Text bold>{block.headers.map(toPlainText).join(" | ")}</Text>
					{block.rows.map((row, i) => (
						<Text key={i}>{row.map(toPlainText).join(" | ")}</Text>
					))}
				</Box>
			)

		case "conversation":
			return (
				<Box key={key} flexDirection="column" marginLeft={2}>
					{block.messages.map((msg, i) => (
						<Text key={i}>
							<Text color={msg.role === "user" ? "green" : "blue"}>
								{msg.role === "user" ? "You: " : "AI: "}
							</Text>
							{toPlainText(msg.text)}
						</Text>
					))}
				</Box>
			)

		case "interactive":
			return (
				<Box key={key} flexDirection="column" marginLeft={2}>
					{block.steps.map((step, i) => (
						<Text key={i}>
							<Text color="yellow">{step.command}</Text>
							<Text dimColor> - {toPlainText(step.description)}</Text>
						</Text>
					))}
				</Box>
			)

		case "command":
			return (
				<Box key={key} flexDirection="column" marginY={0}>
					<Text dimColor>{block.description}</Text>
					<Box paddingX={1}>
						<Text color="green">
							{Array.isArray(block.command)
								? block.command.join("\n")
								: block.command}
						</Text>
					</Box>
					{block.output && (
						<Box paddingX={1}>
							<Text dimColor>{block.output}</Text>
						</Box>
					)}
				</Box>
			)

		default:
			return null
	}
}

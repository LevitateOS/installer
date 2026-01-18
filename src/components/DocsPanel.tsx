import { Box, Text, useInput } from "ink"
import { useState, useEffect } from "react"
import {
	docsNav,
	installContent,
	manualInstallContent,
	recipeCliContent,
	recipesContent,
	type DocsContent,
	type ContentBlock,
} from "@levitate/docs-content"

interface DocsPanelProps {
	focused: boolean
}

// Pre-loaded content map for instant switching
const contentMap: Record<string, DocsContent> = {
	"/docs/install": installContent,
	"/docs/manual-install": manualInstallContent,
	"/docs/recipe": recipeCliContent,
	"/docs/recipes": recipesContent,
}

export function DocsPanel({ focused }: DocsPanelProps) {
	const allItems = docsNav.flatMap((section) => section.items)
	const [selectedIdx, setSelectedIdx] = useState(0)
	const [scrollOffset, setScrollOffset] = useState(0)

	const currentItem = allItems[selectedIdx]
	const content = currentItem ? contentMap[currentItem.href] : null

	useInput(
		(input, key) => {
			if (!focused) return

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
		},
		{ isActive: focused }
	)

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
						<Text bold color="cyan" dimColor={!focused}>
							{section.title}
						</Text>
						{section.items.map((item) => {
							const idx = allItems.findIndex((i) => i.href === item.href)
							const isSelected = idx === selectedIdx
							return (
								<Text
									key={item.href}
									color={isSelected ? "green" : undefined}
									dimColor={!focused && !isSelected}
									inverse={isSelected && focused}
								>
									{isSelected ? "▸ " : "  "}
									{item.title}
								</Text>
							)
						})}
					</Box>
				))}
				{focused && (
					<Text dimColor>↑↓ select | j/k scroll</Text>
				)}
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
				{content.intro}
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
					{block.content}
				</Text>
			)

		case "code":
			return (
				<Box key={key} marginY={0} paddingX={1}>
					<Text color="yellow">{block.content}</Text>
				</Box>
			)

		case "file":
			return (
				<Box key={key} flexDirection="column" marginY={0}>
					<Text dimColor>{block.filename}</Text>
					<Box paddingX={1}>
						<Text color="cyan">{block.content}</Text>
					</Box>
				</Box>
			)

		case "list":
			return (
				<Box key={key} flexDirection="column" marginLeft={2}>
					{block.items.map((item, i) => (
						<Text key={i}>
							{"• "}
							{typeof item === "string" ? item : item.text}
						</Text>
					))}
				</Box>
			)

		case "table":
			return (
				<Box key={key} flexDirection="column" marginY={0}>
					<Text bold>{block.headers.join(" | ")}</Text>
					{block.rows.map((row, i) => (
						<Text key={i}>{row.join(" | ")}</Text>
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
							{msg.text}
						</Text>
					))}
				</Box>
			)

		case "link":
			return (
				<Text key={key} color="blue">
					{block.text}
				</Text>
			)

		case "inline-code":
			return (
				<Text key={key} color="yellow">
					{block.content}
				</Text>
			)

		default:
			return null
	}
}

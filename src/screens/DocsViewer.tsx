import { Box, Text, useInput } from "ink"
import { useState } from "react"
import { docsNav, docsManifest, type DocsContent } from "@levitate/docs-content"

export function DocsViewer() {
	const [selectedSection, setSelectedSection] = useState(0)
	const [selectedItem, setSelectedItem] = useState(0)
	const [content, setContent] = useState<DocsContent | null>(null)
	const [loading, setLoading] = useState(false)

	// Flatten nav items for selection
	const allItems = docsNav.flatMap((section) => section.items)
	const currentItem = allItems[selectedItem]

	useInput((input, key) => {
		if (key.upArrow) {
			setSelectedItem((prev) => Math.max(0, prev - 1))
		}
		if (key.downArrow) {
			setSelectedItem((prev) => Math.min(allItems.length - 1, prev + 1))
		}
		if (key.return && currentItem) {
			loadContent(currentItem.href)
		}
		if (input === "q") {
			setContent(null)
		}
	})

	async function loadContent(href: string) {
		const slug = href.replace("/docs/", "") as keyof typeof docsManifest
		if (slug in docsManifest) {
			setLoading(true)
			const data = await docsManifest[slug]()
			setContent(data)
			setLoading(false)
		}
	}

	if (loading) {
		return (
			<Box padding={1}>
				<Text>Loading...</Text>
			</Box>
		)
	}

	if (content) {
		return <ContentView content={content} />
	}

	return (
		<Box flexDirection="row" height="100%">
			{/* Sidebar */}
			<Box
				flexDirection="column"
				width={30}
				borderStyle="single"
				borderRight
				paddingX={1}
			>
				<Text bold underline>
					Documentation
				</Text>
				<Text> </Text>
				{docsNav.map((section, sIdx) => (
					<Box key={section.title} flexDirection="column">
						<Text bold color="cyan">
							{section.title}
						</Text>
						{section.items.map((item) => {
							const itemIdx = allItems.findIndex((i) => i.href === item.href)
							const isSelected = itemIdx === selectedItem
							return (
								<Text
									key={item.href}
									color={isSelected ? "green" : "white"}
									backgroundColor={isSelected ? "gray" : undefined}
								>
									{isSelected ? "▶ " : "  "}
									{item.title}
								</Text>
							)
						})}
						<Text> </Text>
					</Box>
				))}
				<Text dimColor>↑/↓ Navigate, Enter to view</Text>
			</Box>

			{/* Main content area */}
			<Box flexGrow={1} padding={1}>
				<Text dimColor>Select a document to view</Text>
			</Box>
		</Box>
	)
}

function ContentView({ content }: { content: DocsContent }) {
	return (
		<Box flexDirection="column" padding={1}>
			<Text bold color="cyan">
				{content.title}
			</Text>
			<Text> </Text>
			{content.intro && <Text dimColor>{content.intro}</Text>}
			<Text> </Text>
			{content.sections.slice(0, 5).map((section, i) => (
				<Box key={i} flexDirection="column" marginBottom={1}>
					<Text bold>{section.title}</Text>
					{section.content.slice(0, 2).map((block, j) => {
						if (block.type === "text") {
							return (
								<Text key={j} wrap="wrap">
									{block.content}
								</Text>
							)
						}
						if (block.type === "code") {
							return (
								<Box
									key={j}
									borderStyle="single"
									paddingX={1}
									marginY={1}
								>
									<Text color="yellow">{block.content}</Text>
								</Box>
							)
						}
						return null
					})}
				</Box>
			))}
			<Text> </Text>
			<Text dimColor>Press 'q' to go back</Text>
		</Box>
	)
}

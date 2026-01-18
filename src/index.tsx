#!/usr/bin/env bun
import { render, Box, Text, useInput, useApp } from "ink"
import { DocsPanel } from "./components/DocsPanel.js"

function App() {
	const { exit } = useApp()

	useInput((input, key) => {
		// Ctrl+C to exit
		if (input === "c" && key.ctrl) {
			exit()
		}
	})

	return (
		<Box flexDirection="column" width="100%" height="100%">
			{/* Header */}
			<Box borderStyle="single" paddingX={1} justifyContent="space-between">
				<Text bold color="cyan">
					LevitateOS Documentation
				</Text>
				<Text dimColor>
					↑↓ select | j/k scroll | Ctrl+C exit
				</Text>
			</Box>

			{/* Docs panel (full width) */}
			<Box flexGrow={1} borderStyle="single" borderColor="green">
				<DocsPanel />
			</Box>
		</Box>
	)
}

render(<App />)

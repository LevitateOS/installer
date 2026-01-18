#!/usr/bin/env node
import { render, Box, Text, useInput, useApp } from "ink"
import { useState, useCallback } from "react"
import { Shell } from "./components/Shell.js"
import { DocsPanel } from "./components/DocsPanel.js"

type FocusPane = "shell" | "docs"

function App() {
	const { exit } = useApp()
	const [focus, setFocus] = useState<FocusPane>("shell")

	useInput((input, key) => {
		// Tab to switch focus between panes
		if (key.tab) {
			setFocus((prev) => (prev === "shell" ? "docs" : "shell"))
		}
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
					LevitateOS Installer
				</Text>
				<Text dimColor>
					Tab: switch pane | Ctrl+C: exit
				</Text>
			</Box>

			{/* Main split view */}
			<Box flexGrow={1} flexDirection="row">
				{/* Left: Shell */}
				<Box
					width="50%"
					flexDirection="column"
					borderStyle="single"
					borderColor={focus === "shell" ? "green" : "gray"}
				>
					<Box paddingX={1} borderStyle="single" borderBottom borderTop={false} borderLeft={false} borderRight={false}>
						<Text bold color={focus === "shell" ? "green" : "white"}>
							Shell
						</Text>
					</Box>
					<Shell focused={focus === "shell"} />
				</Box>

				{/* Right: Docs */}
				<Box
					width="50%"
					flexDirection="column"
					borderStyle="single"
					borderColor={focus === "docs" ? "green" : "gray"}
				>
					<Box paddingX={1} borderStyle="single" borderBottom borderTop={false} borderLeft={false} borderRight={false}>
						<Text bold color={focus === "docs" ? "green" : "white"}>
							Documentation
						</Text>
					</Box>
					<DocsPanel focused={focus === "docs"} />
				</Box>
			</Box>
		</Box>
	)
}

render(<App />)

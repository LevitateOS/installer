#!/usr/bin/env node
import { render, Box, Text } from "ink"
import { useState } from "react"
import { DocsViewer } from "./screens/DocsViewer.js"
import { Header } from "./components/Header.js"

type Screen = "docs" | "chat" | "shell"

function App() {
	const [screen, setScreen] = useState<Screen>("docs")

	return (
		<Box flexDirection="column" height="100%">
			<Header currentScreen={screen} onNavigate={setScreen} />
			<Box flexGrow={1}>
				{screen === "docs" && <DocsViewer />}
				{screen === "chat" && (
					<Box padding={1}>
						<Text>Chat screen - coming soon</Text>
					</Box>
				)}
				{screen === "shell" && (
					<Box padding={1}>
						<Text>Shell screen - coming soon</Text>
					</Box>
				)}
			</Box>
		</Box>
	)
}

render(<App />)

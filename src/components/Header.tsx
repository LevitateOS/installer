import { Box, Text } from "ink"

type Screen = "docs" | "chat" | "shell"

interface HeaderProps {
	currentScreen: Screen
	onNavigate: (screen: Screen) => void
}

export function Header({ currentScreen }: HeaderProps) {
	const tabs: { key: Screen; label: string; shortcut: string }[] = [
		{ key: "docs", label: "Docs", shortcut: "1" },
		{ key: "chat", label: "Chat", shortcut: "2" },
		{ key: "shell", label: "Shell", shortcut: "3" },
	]

	return (
		<Box borderStyle="single" paddingX={1}>
			<Text bold color="cyan">
				LevitateOS Installer
			</Text>
			<Text> │ </Text>
			{tabs.map((tab, i) => (
				<Box key={tab.key}>
					{i > 0 && <Text> </Text>}
					<Text
						color={currentScreen === tab.key ? "green" : "white"}
						bold={currentScreen === tab.key}
					>
						[{tab.shortcut}] {tab.label}
					</Text>
				</Box>
			))}
		</Box>
	)
}

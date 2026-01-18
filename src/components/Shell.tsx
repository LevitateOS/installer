import { Box, Text, useInput } from "ink"
import TextInput from "ink-text-input"
import { useState, useCallback } from "react"

interface ShellProps {
	focused: boolean
}

interface HistoryEntry {
	command: string
	output: string
}

export function Shell({ focused }: ShellProps) {
	const [input, setInput] = useState("")
	const [history, setHistory] = useState<HistoryEntry[]>([
		{ command: "", output: "Welcome to LevitateOS Installer Shell\nType 'help' for available commands.\n" },
	])

	const handleSubmit = useCallback((command: string) => {
		const output = executeCommand(command)
		setHistory((prev) => [...prev, { command, output }])
		setInput("")
	}, [])

	return (
		<Box flexDirection="column" flexGrow={1} padding={1}>
			{/* History */}
			<Box flexDirection="column" flexGrow={1} overflowY="hidden">
				{history.map((entry, i) => (
					<Box key={i} flexDirection="column">
						{entry.command && (
							<Text>
								<Text color="green">$ </Text>
								<Text>{entry.command}</Text>
							</Text>
						)}
						{entry.output && (
							<Text wrap="wrap">{entry.output}</Text>
						)}
					</Box>
				))}
			</Box>

			{/* Input */}
			<Box>
				<Text color="green">$ </Text>
				{focused ? (
					<TextInput
						value={input}
						onChange={setInput}
						onSubmit={handleSubmit}
						placeholder="Type a command..."
					/>
				) : (
					<Text dimColor>{input || "Type a command..."}</Text>
				)}
			</Box>
		</Box>
	)
}

function executeCommand(command: string): string {
	const cmd = command.trim().toLowerCase()
	const args = cmd.split(/\s+/)
	const base = args[0]

	switch (base) {
		case "help":
			return `Available commands:
  help          Show this help message
  lsblk         List block devices
  disks         Alias for lsblk
  install       Start installation wizard
  clear         Clear screen
  exit          Exit installer
`

		case "lsblk":
		case "disks":
			return `NAME        SIZE  TYPE  MODEL
sda         500G  disk  Samsung SSD 870
├─sda1      512M  part  (EFI)
└─sda2      499G  part  (Linux)
nvme0n1     1T    disk  WD Black SN850X
`

		case "install":
			return `Starting installation wizard...
Please select a target disk using 'disks' command.
Then run: install <disk>
`

		case "clear":
			return "\x1Bc" // Clear screen escape code

		case "exit":
			return "Use Ctrl+C to exit the installer."

		case "":
			return ""

		default:
			return `Command not found: ${base}\nType 'help' for available commands.`
	}
}

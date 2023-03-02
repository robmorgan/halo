package main

import (
	"fmt"

	"github.com/charmbracelet/lipgloss"
)

var (
	spinnerStyle  = lipgloss.NewStyle().Foreground(lipgloss.Color("63"))
	helpStyle     = lipgloss.NewStyle().Foreground(lipgloss.Color("241")).Margin(1, 0)
	dotStyle      = helpStyle.Copy().UnsetMargins()
	durationStyle = dotStyle.Copy()
	appStyle      = lipgloss.NewStyle().Margin(1, 2, 0, 2)
)

// TODO - render a progress bar for each cue.
// TODO - show active cue count
func (m model) View() string {
	var s string

	s += fmt.Sprintf("Total cues: %d\nBPM: %d\n\n%s Cues processed: %d\n\n", len(m.cueMaster.cues), m.bpm, m.spinner.View(), m.cueMaster.cuesProcessed)

	s += helpStyle.Render("(G)o ([,]) BPM +/-\n\nPress ctrl+c to exit\n")

	if m.quitting {
		s += "\n"
	}
	return appStyle.Render(s)
}

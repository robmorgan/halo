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
	s += fmt.Sprintf("Pending cues: %d\n%s Cues processed: %d\n\nBPM: %d\n\n", len(m.cueMaster.pendingCues), m.spinner.View(), len(m.cueMaster.processedCues), m.bpm)
	s += fmt.Sprintf("Active Cue Count: %d\n\n", len(m.cueMaster.activeCues))
	s += fmt.Sprintf("Frames Sent: %d\n\n", m.framesSent)

	// render progress bars for all active cues
	for i, _ := range m.cueMaster.activeCues {
		s += m.activeProgress[i].ViewAs(m.progress)
	}

	s += helpStyle.Render("(G)o ([,]) BPM +/-\n\nPress ctrl+c to exit\n")

	if m.quitting {
		s += "\n"
	}
	return appStyle.Render(s)
}

package main

import (
	"github.com/charmbracelet/bubbles/spinner"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

type model struct {
	sub           chan struct{} // where we'll receive activity notifications
	bpm           int
	cuesProcessed int // how many cues we've processed
	spinner       spinner.Model
	index         int
	cues          []Cue
	activeCues    []Cue
	processedCues []Cue
	quitting      bool

	// TODO - properties to implement
	// FixtureManager
	// CueMaster
	// MasterTempo
}

func (m model) Init() tea.Cmd {
	return tea.Batch(processCue(m.cues[m.index]), m.spinner.Tick)
}

func newModel() model {
	s := spinner.New()
	s.Style = lipgloss.NewStyle().Foreground(lipgloss.Color("63"))
	return model{
		bpm:     130,
		cues:    getCues(),
		spinner: s,
	}
}

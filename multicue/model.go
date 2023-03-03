package main

import (
	"time"

	"github.com/charmbracelet/bubbles/progress"
	"github.com/charmbracelet/bubbles/spinner"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

type model struct {
	sub            chan struct{} // where we'll receive activity notifications
	bpm            int
	spinner        spinner.Model
	activeProgress []progress.Model // we reuse a pool of progress bars for active cues
	progress       float64
	cueMaster      CueMaster
	quitting       bool

	// TODO - properties to implement
	// FixtureManager
}

func newModel() model {
	s := spinner.New()
	s.Style = lipgloss.NewStyle().Foreground(lipgloss.Color("63"))

	// prepare a pool of progress bars
	pp := make([]progress.Model, 0)
	for i := 0; i < MaxActiveCues; i++ {
		p := progress.New(
			progress.WithDefaultGradient(),
			progress.WithWidth(40),
			progress.WithoutPercentage(),
		)

		pp = append(pp, p)
	}

	// Init the CueMaster
	cm := CueMaster{}

	// Enqueue Cues
	cues := getCues()
	cm.pendingCues = cues

	return model{
		bpm:            130,
		cueMaster:      cm,
		spinner:        s,
		activeProgress: pp,
	}
}

func (m model) Init() tea.Cmd {
	return tea.Batch(tickCmd(), m.spinner.Tick)
}

type tickMsg time.Time

func tickCmd() tea.Cmd {
	return tea.Tick(time.Second*1, func(t time.Time) tea.Msg {
		return tickMsg(t)
	})
}

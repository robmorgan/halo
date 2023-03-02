package main

import (
	"github.com/charmbracelet/bubbles/progress"
	"github.com/charmbracelet/bubbles/spinner"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

type model struct {
	sub chan struct{} // where we'll receive activity notifications
	bpm int

	spinner   spinner.Model
	cueMaster CueMaster
	quitting  bool

	// TODO - properties to implement
	// FixtureManager
}

func (m model) Init() tea.Cmd {
	return tea.Batch(processCue(m.cueMaster.cues[m.cueMaster.index]), m.spinner.Tick)
}

func newModel() model {
	s := spinner.New()
	s.Style = lipgloss.NewStyle().Foreground(lipgloss.Color("63"))

	// init the cue master
	cm := CueMaster{}

	// prepare a pool of progress bars
	for i := 0; i < MaxActiveCues; i++ {
		p := progress.New(
			progress.WithDefaultGradient(),
			progress.WithWidth(40),
			progress.WithoutPercentage(),
		)

		cm.activeProgress = append(cm.activeProgress, p)
	}

	// enqueue cues
	cm.cues = getCues()

	return model{
		bpm:       130,
		cueMaster: cm,
		spinner:   s,
	}
}

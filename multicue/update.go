package main

import (
	"time"

	"github.com/charmbracelet/bubbles/spinner"
	tea "github.com/charmbracelet/bubbletea"
)

type cueProcessedMsg Cue

func (m model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.KeyMsg:
		switch msg.String() {
		case "[":
			m.bpm--
		case "]":
			m.bpm++
		case "ctrl+c":
			m.quitting = true
			return m, tea.Quit
		}
	case cueProcessedMsg:
		//return m, waitForActivity(m.sub) // wait for next event
		// TODO - process the next cue
		return m, nil
	case spinner.TickMsg:
		var cmd tea.Cmd
		m.spinner, cmd = m.spinner.Update(msg)
		return m, cmd
	default:
		return m, nil
	}
	return m, nil
}

func (m model) processNextCue() tea.Cmd {
	// get the target cue
	//x, xs := xs[i], append(xs[:i], xs[i+1:]...)

	// pop a cue off the stack
	cue, pc := m.cueMaster.pendingCues[0], m.cueMaster.pendingCues[1:]
	m.cueMaster.pendingCues = pc

	// This is where you'd do i/o stuff to process the actual cue.
	// In our case we're just pausing for a moment to simulate the process.
	//d := time.Millisecond * time.Duration(rand.Intn(500))
	d := cue.FadeTime

	return tea.Tick(d, func(t time.Time) tea.Msg {
		return cueProcessedMsg("foo")
	})
}

func removeIndex(s []int, index int) []int {
	return append(s[:index], s[index+1:]...)
}

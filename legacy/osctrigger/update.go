package main

import (
	"github.com/charmbracelet/bubbles/spinner"
	tea "github.com/charmbracelet/bubbletea"
)

type playlistProcessedMsg string

func (m model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.KeyMsg:
		switch msg.String() {
		case "g":
			// pop the next playlist off the stack
			var nextPlaylist Playlist
			nextPlaylist, m.pendingPlaylists = m.pendingPlaylists[0], m.pendingPlaylists[1:]
			m.activePlaylist = nextPlaylist
		case "ctrl+c":
			m.quitting = true
			return m, tea.Quit
		}
	case playlistProcessedMsg:
		//return m, waitForActivity(m.sub) // wait for next event
		// TODO - process the next cue
		return m, nil
	case tickMsg:
		//if m.progress.Percent() == 1.0 {
		//	return m, tea.Quit
		//}

		// Note that you can also use progress.Model.SetPercent to set the
		// percentage value explicitly, too.
		//cmd := m.progress.IncrPercent(0.25)
		//for i, _ := range m.cueMaster.activeCues {

		//p := m.activeProgress[i]
		//p.IncrPercent(0.25)
		//fmt.Println("foo")

		//}
		// TODO - get the next frame from all active cues
		// tell all active cues to render the next frame
		return m, tickCmd()

	case spinner.TickMsg:
		var cmd tea.Cmd
		m.spinner, cmd = m.spinner.Update(msg)
		return m, cmd
	default:
		return m, nil
	}
	return m, nil
}

// func (m model) processNextCue() tea.Cmd {
// 	// get the target cue
// 	//x, xs := xs[i], append(xs[:i], xs[i+1:]...)

// 	// pop a cue off the stack
// 	cue, pc := m.cueMaster.pendingCues[0], m.cueMaster.pendingCues[1:]
// 	m.cueMaster.pendingCues = pc

// 	// This is where you'd do i/o stuff to process the actual cue.
// 	// In our case we're just pausing for a moment to simulate the process.
// 	//d := time.Millisecond * time.Duration(rand.Intn(500))
// 	d := cue.FadeTime

// 	return tea.Tick(d, func(t time.Time) tea.Msg {
// 		return cueProcessedMsg("foo")
// 	})
// }

// func removeIndex(s []int, index int) []int {
// 	return append(s[:index], s[index+1:]...)
// }

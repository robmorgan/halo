package main

import (
	"fmt"
	"math/rand"
	"os"
	"time"

	tea "github.com/charmbracelet/bubbletea"
)

var p *tea.Program

func main() {
	rand.Seed(time.Now().UTC().UnixNano())

	if err := tea.NewProgram(newModel()).Start(); err != nil {
		fmt.Println("Error running program:", err)
		os.Exit(1)
	}
}

// func main() {
//

// 	p := tea.NewProgram(model{
// 		sub:     make(chan struct{}),
// 		spinner: spinner.New(),
// 	})

// 	if p.Start() != nil {
// 		fmt.Println("could not start program")
// 		os.Exit(1)
// 	}
// }

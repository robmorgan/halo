package main

import (
	"fmt"
	"math/rand"
	"os"
	"time"

	tea "github.com/charmbracelet/bubbletea"
)

func main() {
	rand.Seed(time.Now().Unix())

	if err := tea.NewProgram(newModel()).Start(); err != nil {
		fmt.Println("Error running program:", err)
		os.Exit(1)
	}
}

// func main() {
// 	rand.Seed(time.Now().UTC().UnixNano())

// 	p := tea.NewProgram(model{
// 		sub:     make(chan struct{}),
// 		spinner: spinner.New(),
// 	})

// 	if p.Start() != nil {
// 		fmt.Println("could not start program")
// 		os.Exit(1)
// 	}
// }

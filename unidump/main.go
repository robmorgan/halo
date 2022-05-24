package main

import (
	"log"

	"github.com/nickysemenza/gola"
)

func main() {
	client, err := gola.New("localhost:9010")
	if err != nil {
		panic("could not create client")
	}
	defer client.Close()

	// dump out DMX on universe 1
	if x, err := client.GetDmx(1); err != nil {
		log.Printf("GetDmx: 1: %v", err)
	} else {
		log.Printf("GetDmx: 1: %v", x.Data)
	}
}

package main

import (
	"fmt"
	"math/rand"
	"os"
	"strconv"
	"strings"
	"time"

	"github.com/hypebeast/go-osc/osc"
)

func indent(str string, indentLevel int) string {
	indentation := strings.Repeat("  ", indentLevel)

	result := ""

	for i, line := range strings.Split(str, "\n") {
		if i != 0 {
			result += "\n"
		}

		result += indentation + line
	}

	return result
}

func debug(packet osc.Packet, indentLevel int) string {
	switch packet := packet.(type) {
	default:
		return "Unknown packet type!"

	case *osc.Message:
		//msg := packet.(*osc.Message)
		return fmt.Sprintf("-- OSC Message: %s", packet)

	case *osc.Bundle:
		//bundle := packet.(*osc.Bundle)

		result := fmt.Sprintf("-- OSC Bundle (%s):", packet.Timetag.Time())

		for i, message := range packet.Messages {
			result += "\n" + indent(
				fmt.Sprintf("-- OSC Message #%d: %s", i+1, message),
				indentLevel+1,
			)
		}

		for _, bundle := range packet.Bundles {
			result += "\n" + indent(debug(bundle, 0), indentLevel+1)
		}

		return result
	}
}

// Debugger is a simple Dispatcher that prints all messages and bundles as they
// are received.
type Debugger struct {
	note     int32
	velocity int32
}

// Dispatch implements Dispatcher.Dispatch by printing the packet received.
func (d *Debugger) Dispatch(packet osc.Packet) {
	if packet != nil {
		fmt.Println(debug(packet, 0) + "\n")

		// TODO - support 5 concurrent notes and velocities
		switch packet := packet.(type) {
		case *osc.Message:
			if len(packet.Arguments) > 0 {
				switch packet.Address {
				case "/Note1":
					d.note = packet.Arguments[0].(int32)
					fmt.Printf("Got Note Val: %d\n", d.note)
				case "/Velocity1":
					d.velocity = packet.Arguments[0].(int32)
					fmt.Printf("Got Velocity Val: %d\n", d.velocity)
					d.triggerPlayOrStop()
				}
			}
		}
	}
}

var playlistMap map[int32]int = map[int32]int{
	66: 2,
	67: 2,
	68: 3,
}

func (d *Debugger) triggerPlayOrStop() error {
	if d.note > 0 {
		playlistId := playlistMap[d.note]
		if d.velocity >= 100 {
			triggerMessage(fmt.Sprintf("/splay/playlist/play/%d", playlistId))
		} else if d.velocity == 0 {
			triggerMessage(fmt.Sprintf("/splay/playlist/stop/%d", playlistId))
		}
	}

	return nil
}

func triggerMessage(address string) error {
	ip := "10.143.28.22"
	port := 8000
	client := osc.NewClient(ip, int(port))
	fmt.Println("Calling address: ", address)
	if err := client.Send(osc.NewMessage(address)); err != nil {
		fmt.Println(err)
	}

	return nil
}

func newMessage(id int32) *osc.Message {
	address := fmt.Sprintf("/splay/playlist/play/%d", id)

	return osc.NewMessage(address)
}

func printUsage() {
	fmt.Printf("Usage: %s PORT\n", os.Args[0])
}

func main() {
	rand.Seed(time.Now().Unix())

	numArgs := len(os.Args[1:])

	if numArgs != 1 {
		printUsage()
		os.Exit(1)
	}

	port, err := strconv.ParseInt(os.Args[1], 10, 32)
	if err != nil {
		fmt.Println(err)
		printUsage()
		os.Exit(1)
	}

	addr := fmt.Sprintf("127.0.0.1:%d", port)

	server := &osc.Server{Addr: addr, Dispatcher: &Debugger{}}

	fmt.Println("### Starting osc-proxy")
	fmt.Printf("Listening via UDP on port %d...\n", port)

	if err := server.ListenAndServe(); err != nil {
		fmt.Println(err)
		os.Exit(1)
	}
}

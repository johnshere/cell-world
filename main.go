package main

import (
	"cellworld/config"
	"cellworld/world"
	"fmt"
)

func main() {
	fmt.Println("Cell World: A cellular automaton simulation with Engine")

	if err := config.InitConfig(); err != nil {
		fmt.Println("failed to initialize configuration:", err)
		return
	}

	world.Run()

}

Type ShutdownWindow
	Field Window
	Field ShutDownButton
End Type

Global Shutdown.ShutdownWindow


; Creates the Updates window
Function CreateShutdownWindow.ShutdownWindow()

	S.ShutdownWindow = New ShutdownWindow
	S\Window = CreateWindow("Shutdown", 530, 500, 165, 100, Desktop(), 1)
	CreateWindow("", 0, 0, 0, 0, 0, 1)

	S\ShutDownButton = CreateButton("Shut Down Server", 10, 25, 100, 25, S\Window)
	
	Return S

End Function
Include "Modules\IO\Image.bb"

Type UpdatesWindow
	Field Window
	Field LockImage.BBImage
	Field UnlockImage.BBImage
	Field ImageBox
	Field LockButton
	Field LockLabel
End Type

Type UpdateFile
	Field Name$, Checksum
End Type

Global Updates.UpdatesWindow

; Loads the files list
Function LoadUpdateFiles()

	F = ReadFile("Data\Server Data\Files.dat")
	If F = 0 Then Return 0

		Files = 0
		While Eof(F) = False
			Files = Files + 1
			U.UpdateFile = New UpdateFile
			U\Name$ = ReadString$(F)
			U\Checksum = ReadInt(F)
		Wend

	CloseFile(F)
	Return Files

End Function

; Creates the Updates window
Function CreateUpdatesWindow.UpdatesWindow()

	U.UpdatesWindow = New UpdatesWindow
	U\Window = CreateWindow("Updates", 530, 10, 300, 450, Desktop(), 1)

	; Load images first using the Image type
	Local redLight.Image = New Image(CurrentDir() + "Data\Server Data\RedLight.bmp")
	Local greenLight.Image = New Image(CurrentDir() + "Data\Server Data\GreenLight.bmp")
	
	; Store the actual BBImage handles
	U\LockImage = Image::load(redLight)
	U\UnlockImage = Image::load(greenLight)

	; Verify images loaded successfully
	If U\LockImage = Null Or U\UnlockImage = Null
		RuntimeError("Failed to load status light images")
	EndIf

	U\LockButton = CreateButton("Unlock Updates Server", 10, ClientHeight(U\Window) - 50, ClientWidth(U\Window) - 20, 25, U\Window)
	;U\LockPanel = CreatePanel(0, 0, 50, 50, U\Window)
	;CentreGadget(U\LockPanel)
	;SetGadgetShape U\LockPanel, GadgetX(U\LockPanel), 300, 50, 50
	;SetPanelImage(U\LockPanel, "Data\Server Data\RedLight.bmp")

	; Create ImageBox with pointer to loaded image
	U\ImageBox = CreateImageBox((GadgetWidth(U\Window) / 2) - 25, 300, 50, 50, U\Window, Ptr U\LockImage)

	L = CreateLabel("Updates Server Status:", 0, 0, 110, 20, U\Window)
	CentreGadget(L)
	SetGadgetShape L, GadgetX(L), 275, 110, 20

	Dat$ = "The updates server is locked. This means that no players" + Chr(10)
	Dat$ = Dat$ + "can join the game or download updates. Any players" + Chr(10)
	Dat$ = Dat$ + "currently in the game will be removed and must wait" + Chr(10)
	Dat$ = Dat$ + "until the server is unlocked before they can rejoin." + Chr(10)
	Dat$ = Dat$ + "This is to allow you to upload a game update without" + Chr(10)
	Dat$ = Dat$ + "players downloading at the same time, and receiving" + Chr(10)
	Dat$ = Dat$ + "old or corrupt files. Make sure you unlock the server" + Chr(10)
	Dat$ = Dat$ + "as soon as you have finished uploading any updates."
	U\LockLabel = CreateLabel(Dat$, 10, 10, 280, 200, U\Window, 3)

	Return U

End Function
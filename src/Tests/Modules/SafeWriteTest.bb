Strict
EnableGC

; Exercise the real SafeWriteOpen / SafeWriteCommit / SafeWriteAbort in
; Logging.bb against the working directory's filesystem. WriteLog needs
; LogMode + MainLog globals to exist; default LogMode=0 keeps it quiet
; so no Data\Logs directory is required.
Global LogMode = 0
Global MainLog = 0

Include "Modules\Logging.bb"

Global testDir$ = CurrentDir$()
Global ProductionPath$ = testDir$ + "safewrite_test.dat"
Global TempPathExpected$ = ProductionPath$ + ".tmp"
Global BakPath$ = ProductionPath$ + ".bak"

Function CleanupTestFiles()
	If FileType(ProductionPath$) = 1 Then DeleteFile(ProductionPath$)
	If FileType(TempPathExpected$) = 1 Then DeleteFile(TempPathExpected$)
	If FileType(BakPath$) = 1 Then DeleteFile(BakPath$)
End Function

; Helper: write payload to path and close the handle. We close locally
; (rather than letting SafeWriteCommit close) because Strict mode can't
; bridge a BBStream local to SafeWriteCommit's untyped Int parameter --
; passing F=0 tells the helper "I already closed it".
Function SeedFile(path$, payload$)
	Local s.BBStream = WriteFile(path$)
	If s = Null Then Return
	WriteString(s, payload$)
	CloseFile(s)
End Function

Function SeedEmptyFile(path$)
	Local s.BBStream = WriteFile(path$)
	If s = Null Then Return
	CloseFile(s)
End Function

Function ReadFileString$(path$)
	Local s.BBStream = ReadFile(path$)
	If s = Null Then Return ""
	Local payload$ = ReadString(s)
	CloseFile(s)
	Return payload$
End Function

; SafeWriteOpen is a pure helper: it just appends .tmp to the final path.
; Pin that contract -- callers rely on the temp name being deterministic
; so cleanup paths (SafeWriteAbort) can target it.
Test testSafeWriteOpenReturnsTempSuffixedPath()
	Local temp$ = SafeWriteOpen$(ProductionPath$)
	Assert(temp$ = ProductionPath$ + ".tmp")
End Test

; Happy path: write to temp, commit promotes the temp into production and
; deletes the temp. No prior production file exists, so no .bak is made.
Test testSafeWriteCommitPromotesTempToProductionOnFirstSave()
	CleanupTestFiles()

	Local temp$ = SafeWriteOpen$(ProductionPath$)
	SeedFile(temp$, "first save payload")

	Local ok% = SafeWriteCommit%(temp$, ProductionPath$, 0)

	Assert(ok = True)
	Assert(FileType(ProductionPath$) = 1)
	Assert(FileType(temp$) <> 1) ; temp consumed
	Assert(FileType(BakPath$) <> 1) ; no prior file -> no .bak

	CleanupTestFiles()
End Test

; When a production file already exists, commit demotes it to .bak before
; promoting the new temp. Lets a crash mid-promote recover the previous
; version.
Test testSafeWriteCommitDemotesPriorProductionToBak()
	CleanupTestFiles()

	SeedFile(ProductionPath$, "previous save")

	Local temp$ = SafeWriteOpen$(ProductionPath$)
	SeedFile(temp$, "newer save")

	Local ok% = SafeWriteCommit%(temp$, ProductionPath$, 0)

	Assert(ok = True)
	Assert(FileType(ProductionPath$) = 1)
	Assert(FileType(BakPath$) = 1) ; previous version preserved
	Assert(FileType(temp$) <> 1)
	Assert(ReadFileString$(BakPath$) = "previous save")
	Assert(ReadFileString$(ProductionPath$) = "newer save")

	CleanupTestFiles()
End Test

; Empty temp = silent WriteFile failure. SafeWriteCommit must refuse to
; promote an empty file, otherwise production would be replaced with 0
; bytes (which is the whole bug the helper exists to prevent).
Test testSafeWriteCommitRefusesEmptyTemp()
	CleanupTestFiles()

	SeedFile(ProductionPath$, "must survive")

	Local temp$ = SafeWriteOpen$(ProductionPath$)
	SeedEmptyFile(temp$)

	Local ok% = SafeWriteCommit%(temp$, ProductionPath$, 0)

	Assert(ok = False)
	Assert(FileType(ProductionPath$) = 1) ; production untouched
	Assert(FileType(temp$) <> 1) ; empty temp deleted by commit
	Assert(ReadFileString$(ProductionPath$) = "must survive")

	CleanupTestFiles()
End Test

; SafeWriteAbort cleans up the temp when the caller decides not to commit
; (e.g. a serialization error mid-write).
Test testSafeWriteAbortRemovesTemp()
	CleanupTestFiles()

	Local temp$ = SafeWriteOpen$(ProductionPath$)
	SeedFile(temp$, "abandoned")

	Assert(FileType(temp$) = 1)
	SafeWriteAbort(temp$)
	Assert(FileType(temp$) <> 1)

	CleanupTestFiles()
End Test

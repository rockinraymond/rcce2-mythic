Strict
EnableGC

Type ActorInstance
	Field Account
End Type

Type QuestLog
	Field EntryName$[499]
	Field EntryStatus$[499]
End Type

Function WriteActorInstance(F, A.ActorInstance)
End Function

Function ReadActorInstance.ActorInstance(F)
	Return Null
End Function

Function FreeActorInstance(A.ActorInstance)
End Function

Include "Modules\AccountsServer.bb"

Test testFindAccountByListIDReturnsMatchingAccount()
	Local firstAccount.Account = New Account
	firstAccount\User$ = "first"
	firstAccount\ListID = 0

	Local secondAccount.Account = New Account
	secondAccount\User$ = "second"
	secondAccount\ListID = 1

	Local thirdAccount.Account = New Account
	thirdAccount\User$ = "third"
	thirdAccount\ListID = 2

	Local found.Account = FindAccountByListID(1)
	Assert(found = secondAccount)
	Assert(found\User$ = "second")

	Delete Each Account
End Test

Test testFindAccountByListIDReturnsNullForInvalidSelection()
	Local firstAccount.Account = New Account
	firstAccount\User$ = "first"
	firstAccount\ListID = 0

	Assert(FindAccountByListID(-1) = Null)
	Assert(FindAccountByListID(7) = Null)

	Delete Each Account
End Test

; FormatAccountListEntry$ should produce the exact display strings the
; Accounts list box has historically used for each combination of GM,
; banned, and logged-on status. These tests pin the format so that future
; refactors of SetLoginStatus / LoadAccounts cannot drift the user-visible
; output without being noticed.

Test testFormatAccountListEntryLoggedOutPlainAccount()
	Assert(FormatAccountListEntry$(False, False, -1, "alice", "alice@example.com") = "alice  (alice@example.com)")
End Test

Test testFormatAccountListEntryLoggedOutBanned()
	Assert(FormatAccountListEntry$(False, True, -1, "alice", "alice@example.com") = "[BAN] alice  (alice@example.com)")
End Test

Test testFormatAccountListEntryLoggedOutGM()
	Assert(FormatAccountListEntry$(True, False, -1, "alice", "alice@example.com") = "[GM] alice  (alice@example.com)")
End Test

Test testFormatAccountListEntryLoggedOutBannedGM()
	Assert(FormatAccountListEntry$(True, True, -1, "alice", "alice@example.com") = "[BAN][GM] alice  (alice@example.com)")
End Test

Test testFormatAccountListEntryLoggedInPlainAccount()
	Assert(FormatAccountListEntry$(False, False, 0, "alice", "alice@example.com") = "* alice  (alice@example.com)")
End Test

Test testFormatAccountListEntryLoggedInBanned()
	Assert(FormatAccountListEntry$(False, True, 3, "alice", "alice@example.com") = "* [BAN] alice  (alice@example.com)")
End Test

Test testFormatAccountListEntryLoggedInGM()
	Assert(FormatAccountListEntry$(True, False, 9, "alice", "alice@example.com") = "* [GM] alice  (alice@example.com)")
End Test

Test testFormatAccountListEntryLoggedInBannedGM()
	Assert(FormatAccountListEntry$(True, True, 5, "alice", "alice@example.com") = "* [BAN][GM] alice  (alice@example.com)")
End Test

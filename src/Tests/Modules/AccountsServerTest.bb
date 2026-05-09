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

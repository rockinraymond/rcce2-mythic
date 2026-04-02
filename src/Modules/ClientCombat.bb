Global LastAttack, AttackTarget
Global CombatDelay
Global DamageInfoStyle

Type BloodSpurt
	Field EmitterEN
	Field Timer
End Type

Type FloatingNumber
	Field EN
	Field Lifespan#
End Type

; Attacks target if the player is able to
Function UpdateCombat()

	; If I have a human target and I'm not riding a mount
	If PlayerTarget > 0 And Me\Attributes\Value[HealthStat] > 0 And AttackTarget = True And Me\Mount = Null
		A.ActorInstance = Object.ActorInstance(PlayerTarget)
		
		If A\Attributes\Value[HealthStat] < 1
			PlayerTarget = 0
			HideEntity(ActorSelectEN)			; Replace through clean function
			DestroyCharInteractionWindow()
			If useClickMovement then ShowEntity(ClickMarkerEN)			; Replace through clean function
			Return
		EndIf
		
		; Get allowed range
		MaxRange# = 4.0
		If Me\Inventory\Items[SlotI_Weapon] <> Null
			If Me\Inventory\Items[SlotI_Weapon]\Item\WeaponClass = WC_Bow
				If Me\Inventory\Items[SlotI_Weapon]\ItemHealth > 0 Then MaxRange# = (Me\Inventory\Items[SlotI_Weapon]\Item\Range#)- 0.5
			EndIf
		EndIf

		; If it's in range
		Dist# = EntityDistance#(Me\CollisionEN, A\CollisionEN)
		If Dist# < MaxRange# + ((A\Actor\Radius# + Me\Actor\Radius#) * 0.05)
			; Stop moving
			Me\DestX# = EntityX#(Me\CollisionEN)
			Me\DestZ# = EntityZ#(Me\CollisionEN)

			; Face target
			PointEntity Me\CollisionEN, A\CollisionEN
			RotateEntity Me\CollisionEN, 0.0, EntityYaw#(Me\CollisionEN) + 180.0, 0.0

			; Attack if enough time elapsed
			If MilliSecs() - LastAttack > CombatDelay + GetActorAttackSpeed(Me)
				; Tell server
				RCE_Send(Connection, PeerToHost, P_AttackActor, RCE_StrFromInt$(A\RuntimeID, 2), True)
				LastAttack = MilliSecs()
			EndIf
		EndIf

		; Chase it
		If Dist# > MaxRange# + ((A\Actor\Radius# + Me\Actor\Radius#) * 0.05) - 2.0
			If CurrentSeq(Me) < Anim_DefaultAttack Or Animating(Me\EN) = False
				SetDestination(Me, EntityX#(A\CollisionEN), EntityZ#(A\CollisionEN), EntityY#(A\CollisionEN))
			EndIf
		EndIf
	EndIf

	; Update blood spurts
	For B.BloodSpurt = Each BloodSpurt
		If MilliSecs() - B\Timer > 600
			RP_KillEmitter(B\EmitterEN, False, False)
			Delete(B)
		EndIf
	Next

End Function

; Loads combat settings from file
Function LoadCombat()

	F = ReadFile("Data\Game Data\Combat.dat")
	If F = 0 Then RuntimeError("Could not open Data\Game Data\Combat.dat!")
		CombatDelay = ReadShort(F)
		DamageInfoStyle = ReadByte(F)
	CloseFile(F)
	LastAttack = MilliSecs()

	; Replace blood texture IDs with RottParticles config handles
	For A.Actor = Each Actor
		Tex = GetTexture(A\BloodTexID, True)
		If Tex > 0
			A\BloodTexID = RP_LoadEmitterConfig("Data\Emitter Configs\Blood.rpc", Tex, Cam)
		Else
			A\BloodTexID = 0
		EndIf
	Next

End Function

; Plays an actor's attack animation
Function AnimateActorAttack(A.ActorInstance)

	If A\Gender = 0
		AS.AnimSet = AnimList(A\Actor\MAnimationSet)
	Else
		AS.AnimSet = AnimList(A\Actor\FAnimationSet)
	EndIf

	; Choose animation and play it
	If A\Inventory\Items[SlotI_Weapon] = Null
		select Rand(1,2)
			case 1
			Anim = FindAnimation(AS, "Default attack")
			case 2
			Anim = FindAnimation(AS, "Right Hand Attack")
		End Select
	Else
		Select A\Inventory\Items[SlotI_Weapon]\Item\WeaponClass
			Case WC_Dagger 
				select Rand(1,3)
					case 1
					Anim = FindAnimation(AS, "Two hand Attack")
					case 2
					Anim = FindAnimation(AS, "Two Hand 2")
					case 3
					Anim = FindAnimation(AS, "Two Hand 3")
					End Select
			Case WC_Polearm 
				select Rand(1,3)
					case 1
					Anim = FindAnimation(AS, "Two hand 3")
					case 2
					Anim = FindAnimation(AS, "Two Hand Attack")
					case 3
					Anim = FindAnimation(AS, "Two hand 3")
					End Select
			Case WC_Bow: Anim = FindAnimation(AS, "Bow Attack")
			Default
				select Rand(1,2)
					case 1
					Anim = FindAnimation(AS, "Two hand Attack")
					case 2
					Anim = FindAnimation(AS, "Two Hand 2")
					End Select
		End Select
	EndIf
	PlayAnimation(A, 3, 1, Anim, False)

End Function

; Plays an actor's parry animation
Function AnimateActorParry(A.ActorInstance)

	; Choose animation and play it
	If A\Inventory\Items[SlotI_Shield] <> Null
		Anim = Anim_ShieldParry
	ElseIf A\Inventory\Items[SlotI_Weapon] = Null
		Anim = Anim_DefaultParry
	Else
		Select A\Inventory\Items[SlotI_Weapon]\Item\WeaponType
			Case W_OneHand : Anim = Anim_RightParry
			Case W_TwoHand : Anim = Anim_TwoHandParry
			Case W_Ranged : Anim = Anim_DefaultParry
		End Select
	EndIf
	PlayAnimation(A, 3, 0.5, Anim, False)

End Function

; Displays a combat damage message
Function CombatDamageOutput(AI.ActorInstance, Amount, DType$)

	; Chat message
	If DamageInfoStyle = 2
		Name$ = Trim$(AI\Name$)
		If Name$ = "" Then Name$ = AI\Actor\Race$
		; You hit him
		If Amount > 0
			Output(LanguageString$(LS_YouHit) + " " + Name$ + " " + LanguageString$(LS_For) + " " + Str$(Amount) + " " + DType$ + " " + LanguageString$(LS_DamageWow), 0, 255, 0)
		; He hit you
		ElseIf Amount < 0
			Output(Name$ + " " + LanguageString$(LS_HitsYou) + " " + Str$(-Amount) + " " + DType$ + " " + LanguageString$(LS_DamageWow), 255, 0, 0)
		; Miss
		; Else
		; 	; He missed
		; 	If DType$ = "1"
		; 		Output(Name$ + " " + LanguageString$(LS_AttacksYouMisses), 0, 0, 255)
		; 	; You missed
		; 	Else
		; 		Output(LanguageString$(LS_YouAttack) + " " + Name$ + " " + LanguageString$(LS_AndMiss), 0, 0, 255)
		; 	EndIf
		EndIf
	; Floating number
	ElseIf DamageInfoStyle = 3
		; He hit you
		If Amount < 0
			CreateFloatingNumber(Me, Amount, 255, 0, 0)
		; You hit him
		ElseIf Amount > 0
			CreateFloatingNumber(AI, -Amount, 50, 255, 0)
		EndIf
	EndIf

End Function

Function CombatDamageOutputOthers(Attacker.ActorInstance, Defender.ActorInstance,  Amount, DType$, Alignment)

	R = 225
	G = 225
	B = 255
	;determine color of text based on if party members and or pets are in combat, if not the messages will be yellow/neutral
	If Alignment = 2
		R = 0
		G = 225
		B = 0
	ElseIf Alignment = 3
		R = 225
		G = 0
		B = 0
	EndIf
	; Chat message
	If DamageInfoStyle = 2
		AttackerName$ = Trim$(Attacker\Name$)
		If AttackerName$ = "" Then AttackerName$ = Attacker\Actor\Race$
		DefenderName$ = Trim$(Defender\Name$)
		If DefenderName$ = "" Then DefenderName$ = Defender\Actor\Race$
		; You hit him
		If Amount > 0
			Output(AttackerName$ + " hit " + DefenderName$ + " " + LanguageString$(LS_For) + " " + Str$(Amount) + " " + DType$ + " " + LanguageString$(LS_DamageWow), R, G, B)
		EndIf
	; Floating number
	ElseIf DamageInfoStyle = 3
		; He hit you
	
			CreateFloatingNumber(Defender, Amount, R, G, B)
		
	EndIf

End Function

; Creates a floating number
Function CreateFloatingNumber(AI.ActorInstance, Amount, R, G, B)

	F.FloatingNumber = New FloatingNumber
	F\EN = GY_Create3DText(0.0, 0.0, 1.0, 1.0, Len(Str$(Amount)), GY_TitleFont, 0, 0)
	GY_Set3DText(F\EN, Str$(Amount))
	ScaleEntity(F\EN, -0.45 * Len(Str$(Amount)), 0.75, 1, True)
	EntityBlend(F\EN, 3)
	EntityColor(F\EN, R, G, B)
	If AI\NametagEN <> 0
		PositionEntity F\EN, EntityX#(AI\CollisionEN), EntityY#(AI\NametagEN, True), EntityZ#(AI\CollisionEN)
	Else
		PositionEntity F\EN, EntityX#(AI\CollisionEN), EntityY#(AI\CollisionEN) + (MeshHeight#(AI\EN) * 0.025), EntityZ#(AI\CollisionEN)
	EndIf

End Function

; Updates all floating numbers
Function UpdateFloatingNumbers()

	For F.FloatingNumber = Each FloatingNumber
		; Move
		TranslateEntity F\EN, 0, 0.1 * Delta#, 0
		PointEntity F\EN, Cam

		; Update lifespan
		F\Lifespan# = F\Lifespan# + Delta#
		If F\Lifespan# > 50.0
			FreeEntity F\EN
			Delete F
		EndIf
	Next

End Function

Function PlayActorWeaponSound(Attacker.ActorInstance,Target.ActorInstance)

		WeaponType = 0
		If Attacker\Inventory\Items[SlotI_Weapon] <> Null
			WeaponType = Attacker\Inventory\Items[SlotI_Weapon]\Item\WeaponClass
		EndIf

		Result = 58
		Select WeaponType
			Case WC_Sword
				Result = Rand(56,57)
			Case WC_Dagger
				Result = Rand(56,57)
			Case WC_Blunt
				Result = Rand(60,61)
			Case WC_Axe
				Result = Rand(56,57)
			Case WC_Bow
				Result = 55
			Case WC_Polearm
				Result = Rand(56,57)
			Case WC_Staff
				Result = Rand(60,61)
			Case WC_Wand 
				Result = 9
			Default
				Result = Rand(58,59)
		End Select
		
		EN = FindChild(Target\EN, "Head")
		If EN = 0 Then EN = Target\EN
		If Target <> Null
			EmitSound(GetSound(Result), EN)
		EndIf
End Function
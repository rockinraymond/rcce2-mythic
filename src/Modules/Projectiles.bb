; Describes a projectile
Dim ProjectileList.Projectile(5000)
Type Projectile
	Field ID, Name$
	Field MeshID
	Field Emitter1$, Emitter1TexID
	Field Emitter2$, Emitter2TexID
	Field Homing, DamageDie
	Field Attribute$, Skill$
	Field Damage, DamageType, SaveType
	Field Speed
End Type

; Creates a new blank projectile
Function CreateProjectile.Projectile()

	For ID = 0 To 5000
		If ProjectileList(ID) = Null
			P.Projectile = New Projectile
			P\ID = ID
			ProjectileList(P\ID) = P
			Exit
		EndIf
	Next

	Return P

End Function

; Loads all projectiles from a file and returns how many were loaded
Function LoadProjectiles(Filename$)

	Local Projectiles = 0

	F = ReadFile(Filename$)
	If F = 0 Then Return -1

		While Not Eof(F)
			P.Projectile = New Projectile
			P\ID = ReadShort(F)
			ProjectileList(P\ID) = P
			P\Name$ = ReadString$(F)
			P\MeshID = ReadShort(F)
			P\Emitter1$ = ReadString$(F)
			P\Emitter2$ = ReadString$(F)
			P\Emitter1TexID = ReadShort(F)
			P\Emitter2TexID = ReadShort(F)
			P\Homing = ReadByte(F)
			P\DamageDie = ReadByte(F)
			P\Attribute$ = ReadString$(F)
			P\Skill$ = ReadString$(F)
			P\Damage = ReadShort(F)
			P\DamageType = ReadShort(F)
			P\SaveType = ReadShort(F)
			P\Speed = ReadByte(F)

			Projectiles = Projectiles + 1
		Wend

	CloseFile F
	Return Projectiles

End Function

; Saves all loaded projectiles to a file
Function SaveProjectiles(Filename$)

	F = WriteFile(Filename$)
	If F = 0 Then Return False

		For P.Projectile = Each Projectile
			WriteShort F, P\ID
			WriteString F, P\Name$
			WriteShort F, P\MeshID
			WriteString F, P\Emitter1$
			WriteString F, P\Emitter2$
			WriteShort F, P\Emitter1TexID
			WriteShort F, P\Emitter2TexID
			WriteByte F, P\Homing
			WriteByte F, P\DamageDie
			WriteString F, P\Attribute$
			WriteString F, P\Skill$
			WriteShort F, P\Damage
			WriteShort F, P\DamageType
			WriteShort F, P\SaveType
			WriteByte F, P\Speed
		Next

	CloseFile F
	Return True

End Function

; Finds a projectile by name
Function FindProjectile(Name$)

	Name$ = Upper$(Name$)
	For P.Projectile = Each Projectile
		If Upper$(P\Name$) = Name$ Then Return P\ID
	Next
	Return -1

End Function
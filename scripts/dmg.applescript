on run (volumeName)
	tell application "Finder"
		tell disk (volumeName as string)
			open

			set theXOrigin to 10
			set theYOrigin to 60
			set theWidth to 500
			set theHeight to 200

			set theBottomRightX to (theXOrigin + theWidth)
			set theBottomRightY to (theYOrigin + theHeight)
			set dsStore to "\"" & "/Volumes/" & volumeName & "/" & ".DS_STORE\""

			tell container window
				set current view to icon view
				set toolbar visible to false
				set statusbar visible to false
				set the bounds to {theXOrigin, theYOrigin, theBottomRightX, theBottomRightY}
				set statusbar visible to false
			end tell

			set opts to the icon view options of container window
			tell opts
				set icon size to 128
				set text size to 16
				set arrangement to not arranged
			end tell

			set position of item "Applications" to {0, 0}

			close
			open
			delay 1

			tell container window
				set statusbar visible to false
				set the bounds to {theXOrigin, theYOrigin, theBottomRightX - 10, theBottomRightY - 10}
			end tell
		end tell

		delay 1

		tell disk (volumeName as string)
			tell container window
				set statusbar visible to false
				set the bounds to {theXOrigin, theYOrigin, theBottomRightX, theBottomRightY}
			end tell
		end tell

		delay 3

		set waitTime to 0
		set ejectMe to false
		repeat while ejectMe is false
			delay 1
			set waitTime to waitTime + 1

			if (do shell script "[ -f " & dsStore & " ]; echo $?") = "0" then set ejectMe to true
		end repeat
	end tell
end run

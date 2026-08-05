# Emacs menus

This page is a teference listing of emacs menus. 

Vix considers these as suggestions and ideas. NOT must-haves.

## File menu

```txt
Open File...
Open File In Project...         C-x p f
Open Directory...               C-x d
Open Project Directory          C-x p D
Insert File...                  C-x i
Close
--
Save                            C-x C-s
Save As...                      C-x C-w
Revert Buffer                   s-u
Recover Crashed Session
--
New Window Below                C-x 2
New Window on Right             C-x 3
Remove Other Windows            C-x 1
--
New Frame                       C-x 5 2
New Frame on Display Server...
New Frame on Monitor...
Delete Frame                    C-x 5 0
Undelete Frame                  C-x 5 u
[ ] Allow Undeleting Frames
--
New Tab                         C-x t 2
Close Tab                       C-x t 0
--
Print >
--
Quit                            C-x C-c
```

## Edit menu

```txt
Undo                  C-x u
Redo                  C-M-_
--
Cut                   C-w
Copy                  s-c
Paste                 C-y   rk.
Select and Paste >
Clear
Select All            C-x h
--
Search >
Incremental Search >
Replace >
Go To >
Bookmarks >
--
Fill
Spell >
Execute Command       M-x
```


## Options menu

```txt
[X] Highlight Active Region
[X] Highlight Matching Parentheses
--
Line Wrapping in This Buffer >
Default Search Options >
[ ] Cut/Paste with C-x/C-c/C-v (CUA Mode)
--
[X] Use Directory Names in Buffer Names
[ ] Save Place in Files between Sessions
[ ] Save State between Sessions
--
[X] Blink Cursor
--
[ ] Enter Debugger on Error
[ ] Enter Debugger on Quit/C-g
--
Multilingual Environment >
--
Show/Hide >
--
Save Options
Manage Emacs Packages
Customize Emacs >
```

## Buffer menu

```txt
file.txt
directory %
*scratch*
*Messages*
--
Next Buffer                  C-x <right>
Previous Buffer              C-x <left>
Select Named Buffer...       C-x b
List All Buffers             C-x C-b
Select Buffer In Project...  C-x p b
List Buffers In Project...   C-x p C-b
```

## Tools menu

```txt
Search Files (Grep)...
Recursive Grep...
Shell Commands >
Compile...
Compile Project...               C-x p c
Debugger (GDB)...
[ ] Project Support (EDE)
Project >
Language Server Support (Eglot)
[ ] Source Code Parsers (Semantic)
--
Spell Checking >
--
Compare (Ediff) >
Merge >
Apply Patch >
--
Version Control >
--
Read Net News
Read Mail
Compose New Mail                 C-x m
Directory Servers >
Browse the Web...
--
Calendar
Programmable Calculator
Simple Calculator
--
Encryption/Decryption >
--
Games >
```

## Table menu

```txt
Align                            C-c C-c
Next Field                       TAB
Previous Field                   <backtab>
Next Row                         RET
--
Blank Field
Edit Field                       C-c `
Copy Field from Above            S-RET
--
Column >
Row >
Rectangle >
--
Calculate >
[ ] Debug Formulas                   C-c {
[ ] Show Col/Row Numbers             C-c }
--
Create
Convert Region
Import from File
Export to File
--
Create/Convert from/to table.el  C-c ~
--
Plot >
```

## Org menu

```
Show/Hide >
--
New Heading
Navigate Headings >
Edit Structure >
Editing >
Archive >
--
Hyperlinks >
--
TODO Lists >
TAGS and Properties >
Dates and Scheduling >
Logging work >
--
Agenda Command...             C-c o a
Set Restriction Lock          C-c C-x <
File List for Agenda >
Special views current file >
--
Export/Publish...             C-c C-e
LaTeX >
--
Documentation >
Customize >
Send bug report
--
Refresh/Reload >
```

## Text menu

```txt
 Center Line
 Center Paragraph
 Center Region
--
[ ] Paragraph Indent
[ ] Auto Fill
```

## Help menu

```txt
Emacs Tutorial                       C-h t
Emacs Tutorial (choose language)...
Emacs FAQ                            C-h C-f
Emacs News                           C-h n
Emacs Known Problems                 C-h C-p
How to Report a Bug
Send Bug Report...
Emacs Psychotherapist
--
Search Documentation >
Describe >
Read the Emacs Manual                C-h r
More Manuals >
Search Built-in Packages             C-h p
Finding Extra Packages               C-h C-e
--
Getting New Versions                 C-h C-o
Copying Conditions                   C-h C-c
(Non)Warranty                        C-h C-w
--
About Emacs                          C-h C-a
About GNU                            C-h g
```
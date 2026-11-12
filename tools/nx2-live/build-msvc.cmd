@echo off
rem Builds nx2-live.dll (for emulators built with MSVC and Qt 6.8) and nx2-inject.exe into out\msvc.
setlocal
set HERE=%~dp0
set QT=%HERE%..\..\target\qt\6.8.3\msvc2022_64
set OUT=%HERE%out\msvc
call "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat" >nul || exit /b 1
if not exist "%OUT%" mkdir "%OUT%"
cl /nologo /O2 /MD /EHsc /std:c++17 /permissive- /Zc:__cplusplus /utf-8 /DUNICODE /D_UNICODE /DQT_NO_DEBUG ^
  /I"%QT%\include" /I"%QT%\include\QtCore" /I"%QT%\include\QtGui" /I"%QT%\include\QtWidgets" ^
  /Fo"%OUT%\\" "%HERE%live.cpp" /LD /Fe"%OUT%\nx2-live.dll" ^
  /link "%QT%\lib\Qt6Core.lib" "%QT%\lib\Qt6Gui.lib" "%QT%\lib\Qt6Widgets.lib" user32.lib || exit /b 1
cl /nologo /O2 /MT /EHsc /utf-8 /DUNICODE /D_UNICODE /Fo"%OUT%\\" "%HERE%inject.cpp" /Fe"%OUT%\nx2-inject.exe" || exit /b 1

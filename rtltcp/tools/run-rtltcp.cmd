@echo off
rem rsp1-rtltcp を「低整合性ラベルの付いていない」場所へコピーしてから起動する。
rem SDRplay API は中整合性の共有メモリ (Global\sdrSrvComShMem) を使うため、
rem 低整合性ラベルが付いた exe から起動すると sdrplay_api_Open が失敗する。
setlocal
set HERE=%~dp0
set DESTDIR=%LOCALAPPDATA%\rsp1-rtltcp
set SRC=
for %%D in ("%HERE%..\..\target\release" "%HERE%..\..\target\debug" "%HERE%..\..\..\rtltcp-target\release" "%HERE%..\..\..\rtltcp-target\debug") do call :try "%%~fD"
if "%SRC%"=="" goto :nosrc

set RUN=%SRC%
mkdir "%DESTDIR%" 2>nul
copy /Y "%SRC%" "%DESTDIR%\rsp1-rtltcp.exe" >nul 2>&1
if exist "%DESTDIR%\rsp1-rtltcp.exe" set RUN=%DESTDIR%\rsp1-rtltcp.exe

:label
icacls "%RUN%" 2>nul | findstr /i "Mandatory" >nul
if errorlevel 1 goto :run
echo [warn] "%RUN%" に低整合性ラベルが付いています。SDRplay API は開けません。
echo        通常のターミナル (サンドボックス外) から実行し直してください。

:run
echo 実行: "%RUN%" %*
"%RUN%" %*
set RC=%errorlevel%
endlocal & exit /b %RC%

:try
if not "%SRC%"=="" exit /b 0
if exist "%~1\rsp1-rtltcp.exe" set "SRC=%~1\rsp1-rtltcp.exe"
exit /b 0

:nosrc
echo [error] rsp1-rtltcp.exe が見つかりません。先にビルドしてください:
echo         cd /d E:\github\rsp1-fm  ^&^&  cargo build --release -p rsp1-rtltcp
endlocal
exit /b 1
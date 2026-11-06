# Switch 2 Manager : jeux, émulateurs, Steam, cloud et réglages dans une seule fenêtre.
$ErrorActionPreference = 'Stop'
$script:Root = Split-Path $PSScriptRoot -Parent
Add-Type -AssemblyName PresentationFramework, PresentationCore, WindowsBase, System.Windows.Forms
. (Join-Path $PSScriptRoot 'Core.ps1')

[xml]$xaml = @'
<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
        Title="Switch 2 Manager" Width="960" Height="660" MinWidth="820" MinHeight="540" WindowStartupLocation="CenterScreen"
        Background="#F4F5F8" FontFamily="Segoe UI" FontSize="14" Foreground="#1D2230">
  <Window.Resources>
    <SolidColorBrush x:Key="Accent" Color="#E4002B"/>
    <Style x:Key="Primary" TargetType="Button">
      <Setter Property="Foreground" Value="White"/><Setter Property="Background" Value="#E4002B"/><Setter Property="Padding" Value="16,7"/><Setter Property="Cursor" Value="Hand"/><Setter Property="FontWeight" Value="SemiBold"/>
      <Setter Property="Template"><Setter.Value><ControlTemplate TargetType="Button">
        <Border x:Name="B" Background="{TemplateBinding Background}" CornerRadius="7" Padding="{TemplateBinding Padding}"><ContentPresenter HorizontalAlignment="Center" VerticalAlignment="Center"/></Border>
        <ControlTemplate.Triggers><Trigger Property="IsMouseOver" Value="True"><Setter TargetName="B" Property="Opacity" Value="0.86"/></Trigger><Trigger Property="IsEnabled" Value="False"><Setter TargetName="B" Property="Opacity" Value="0.35"/></Trigger></ControlTemplate.Triggers>
      </ControlTemplate></Setter.Value></Setter>
    </Style>
    <Style x:Key="Secondary" TargetType="Button" BasedOn="{StaticResource Primary}">
      <Setter Property="Foreground" Value="#1D2230"/><Setter Property="Background" Value="#E3E6EC"/><Setter Property="FontWeight" Value="Normal"/>
    </Style>
    <Style x:Key="Nav" TargetType="RadioButton">
      <Setter Property="Foreground" Value="#C8CCD6"/><Setter Property="Cursor" Value="Hand"/><Setter Property="FontSize" Value="15"/>
      <Setter Property="Template"><Setter.Value><ControlTemplate TargetType="RadioButton">
        <Border x:Name="B" Background="Transparent" CornerRadius="8" Padding="16,11" Margin="10,2"><ContentPresenter/></Border>
        <ControlTemplate.Triggers><Trigger Property="IsMouseOver" Value="True"><Setter TargetName="B" Property="Background" Value="#2C3140"/></Trigger><Trigger Property="IsChecked" Value="True"><Setter TargetName="B" Property="Background" Value="#E4002B"/><Setter Property="Foreground" Value="White"/></Trigger></ControlTemplate.Triggers>
      </ControlTemplate></Setter.Value></Setter>
    </Style>
    <Style x:Key="Card" TargetType="Border"><Setter Property="Background" Value="White"/><Setter Property="CornerRadius" Value="10"/><Setter Property="Padding" Value="16,12"/><Setter Property="Margin" Value="0,0,0,10"/><Setter Property="BorderBrush" Value="#E3E6EC"/><Setter Property="BorderThickness" Value="1"/></Style>
    <Style x:Key="H1" TargetType="TextBlock"><Setter Property="FontSize" Value="24"/><Setter Property="FontWeight" Value="SemiBold"/><Setter Property="Margin" Value="0,0,0,4"/></Style>
    <Style x:Key="Sub" TargetType="TextBlock"><Setter Property="Foreground" Value="#667085"/><Setter Property="TextWrapping" Value="Wrap"/><Setter Property="Margin" Value="0,0,0,14"/></Style>
    <Style x:Key="Field" TargetType="TextBox"><Setter Property="Padding" Value="6,5"/><Setter Property="Margin" Value="0,3,0,10"/></Style>
    <Style x:Key="Label" TargetType="TextBlock"><Setter Property="FontWeight" Value="SemiBold"/><Setter Property="Margin" Value="0,2,0,0"/></Style>
  </Window.Resources>
  <Grid>
    <Grid.ColumnDefinitions><ColumnDefinition Width="210"/><ColumnDefinition Width="*"/></Grid.ColumnDefinitions>
    <Grid.RowDefinitions><RowDefinition Height="*"/><RowDefinition Height="Auto"/></Grid.RowDefinitions>
    <Border Grid.RowSpan="2" Background="#1B1E28">
      <StackPanel>
        <TextBlock Text="Switch 2" Foreground="White" FontSize="23" FontWeight="Bold" Margin="26,24,0,0"/>
        <TextBlock Text="Manager" Foreground="#8B91A1" FontSize="14" Margin="26,0,0,22"/>
        <RadioButton x:Name="NavGames" Style="{StaticResource Nav}" Content="Jeux" IsChecked="True"/>
        <RadioButton x:Name="NavEmus" Style="{StaticResource Nav}" Content="Émulateurs"/>
        <RadioButton x:Name="NavSteam" Style="{StaticResource Nav}" Content="Steam"/>
        <RadioButton x:Name="NavCloud" Style="{StaticResource Nav}" Content="Cloud"/>
        <RadioButton x:Name="NavSettings" Style="{StaticResource Nav}" Content="Réglages"/>
      </StackPanel>
    </Border>
    <Grid x:Name="Welcome" Grid.ColumnSpan="2" Grid.RowSpan="2" Panel.ZIndex="10" Background="#F4F5F8" Visibility="Collapsed">
      <ScrollViewer VerticalScrollBarVisibility="Auto"><StackPanel MaxWidth="720" Margin="24,28">
        <TextBlock Text="Bienvenue dans Switch 2 Manager" FontSize="27" FontWeight="SemiBold"/>
        <TextBlock Style="{StaticResource Sub}" Margin="0,4,0,18" Text="Quelques réglages pour commencer. Tout reste modifiable ensuite. Tes clés et tes jeux ne sont fournis par aucun outil : ils viennent de ta console."/>
        <Border Style="{StaticResource Card}"><StackPanel>
          <TextBlock Style="{StaticResource Label}" FontSize="16" Text="1. Données déjà présentes sur ce PC"/>
          <TextBlock x:Name="WelcomeFoundText" Foreground="#667085" TextWrapping="Wrap" Margin="0,4,0,6"/>
          <StackPanel x:Name="WelcomeFound"/>
        </StackPanel></Border>
        <Border Style="{StaticResource Card}"><StackPanel>
          <TextBlock Style="{StaticResource Label}" FontSize="16" Text="2. Où ranger tes sauvegardes et ta configuration ?" Margin="0,0,0,6"/>
          <RadioButton x:Name="WelcomePortable" GroupName="Welcome" Content="Portable : à côté de l'outil, rien sur C: (idéal sur un disque externe)" Margin="0,0,0,4"/>
          <RadioButton x:Name="WelcomeLocal" GroupName="Welcome" Content="Installé : sur ce PC, dans AppData"/>
          <TextBlock x:Name="WelcomeLocationNote" Foreground="#667085" TextWrapping="Wrap" Margin="0,6,0,0"/>
        </StackPanel></Border>
        <Border Style="{StaticResource Card}"><StackPanel>
          <TextBlock Style="{StaticResource Label}" FontSize="16" Text="3. Ce qu'il faut pour jouer" Margin="0,0,0,6"/>
          <TextBlock x:Name="WelcomeStatus" TextWrapping="Wrap" Margin="0,0,0,8"/>
          <StackPanel Orientation="Horizontal">
            <Button x:Name="WelcomePrereq" Style="{StaticResource Secondary}" Content="Installer clés et firmware" Margin="0,0,8,0"/>
            <Button x:Name="WelcomeInstall" Style="{StaticResource Secondary}" Content="Installer Lichii"/>
          </StackPanel>
        </StackPanel></Border>
        <TextBlock Foreground="#667085" TextWrapping="Wrap" Margin="2,0,0,12" Text="Le cloud, Steam et les images des jeux s'activent ensuite depuis leurs pages."/>
        <DockPanel><TextBlock x:Name="WelcomeMessage" Foreground="#475067" TextWrapping="Wrap" VerticalAlignment="Center" Width="520"/><Button x:Name="WelcomeDone" Style="{StaticResource Primary}" Content="Commencer" HorizontalAlignment="Right"/></DockPanel>
      </StackPanel></ScrollViewer>
    </Grid>
    <Grid x:Name="Pages" Grid.Column="1" Margin="28,24,28,10">
      <ScrollViewer x:Name="PageGames" VerticalScrollBarVisibility="Auto"><StackPanel>
        <DockPanel><Button x:Name="BtnArt" DockPanel.Dock="Right" Style="{StaticResource Secondary}" Content="Récupérer les images" VerticalAlignment="Top"/><TextBlock Style="{StaticResource H1}" Text="Jeux"/></DockPanel>
        <TextBlock Style="{StaticResource Sub}" Text="Choisis l'émulateur de chaque jeu, puis lance-le. Le choix est retenu."/>
        <Border Style="{StaticResource Card}"><DockPanel>
          <Button x:Name="BtnPrereq" DockPanel.Dock="Right" Style="{StaticResource Secondary}" Content="Installer ce qui manque"/>
          <TextBlock x:Name="SetupText" VerticalAlignment="Center" TextWrapping="Wrap"/>
        </DockPanel></Border>
        <StackPanel x:Name="GamesList"/>
      </StackPanel></ScrollViewer>
      <ScrollViewer x:Name="PageEmus" Visibility="Collapsed" VerticalScrollBarVisibility="Auto"><StackPanel>
        <DockPanel><Button x:Name="BtnRefresh" DockPanel.Dock="Right" Style="{StaticResource Secondary}" Content="Actualiser" VerticalAlignment="Top"/><TextBlock Style="{StaticResource H1}" Text="Émulateurs"/></DockPanel>
        <TextBlock Style="{StaticResource Sub}" Text="Compare ta version à la dernière publiée. L'ancienne version est gardée dans Old Versions, tes données ne changent pas."/>
        <StackPanel x:Name="EmusList"/>
        <Border Style="{StaticResource Card}" Margin="0,8,0,10"><StackPanel>
          <TextBlock Style="{StaticResource Label}" FontSize="16" Text="Ajouter un émulateur" Margin="0,0,0,6"/>
          <TextBlock Style="{StaticResource Label}" Text="Nom"/><TextBox x:Name="AddName" Style="{StaticResource Field}"/>
          <TextBlock Style="{StaticResource Label}" Text="Dépôt GitHub (propriétaire/projet ou adresse)"/><TextBox x:Name="AddRepo" Style="{StaticResource Field}"/>
          <TextBlock Style="{StaticResource Label}" Text="Filtre du fichier à télécharger (facultatif, expression régulière)"/><TextBox x:Name="AddAsset" Style="{StaticResource Field}"/>
          <StackPanel Orientation="Horizontal">
            <Button x:Name="BtnAddGitHub" Style="{StaticResource Primary}" Content="Ajouter depuis GitHub"/>
            <Button x:Name="BtnAddLocal" Style="{StaticResource Secondary}" Content="Ajouter un .exe local" Margin="8,0,0,0"/>
          </StackPanel>
          <TextBlock Foreground="#667085" TextWrapping="Wrap" Margin="0,8,0,0" Text="N'ajoute que des sources de confiance : un émulateur est un programme qui s'exécute sur ton PC."/>
        </StackPanel></Border>
      </StackPanel></ScrollViewer>
      <ScrollViewer x:Name="PageSteam" Visibility="Collapsed" VerticalScrollBarVisibility="Auto"><StackPanel>
        <TextBlock Style="{StaticResource H1}" Text="Steam"/>
        <TextBlock Style="{StaticResource Sub}" Text="Coche les jeux à afficher dans ta bibliothèque. Tes autres raccourcis ne changent pas. Après un déplacement du dossier, clique simplement sur Enregistrer."/>
        <Border Style="{StaticResource Card}"><StackPanel>
          <TextBlock Style="{StaticResource Label}" Text="Compte Steam"/>
          <ComboBox x:Name="SteamAccount" Margin="0,4,0,12" Padding="6,5"/>
          <TextBlock Style="{StaticResource Label}" Text="Jeux dans Steam"/>
          <StackPanel x:Name="SteamGames" Margin="0,6,0,10"/>
          <TextBlock x:Name="SteamInfo" Foreground="#667085" TextWrapping="Wrap" Margin="0,0,0,10"/>
          <Button x:Name="BtnSteamSave" Style="{StaticResource Primary}" Content="Enregistrer dans Steam" HorizontalAlignment="Left"/>
        </StackPanel></Border>
      </StackPanel></ScrollViewer>
      <ScrollViewer x:Name="PageCloud" Visibility="Collapsed" VerticalScrollBarVisibility="Auto"><StackPanel>
        <TextBlock Style="{StaticResource H1}" Text="Cloud"/>
        <TextBlock Style="{StaticResource Sub}" Text="Sauvegardes, profil et configuration seulement. Les clés et le firmware ne sont jamais envoyés."/>
        <Border Style="{StaticResource Card}"><StackPanel>
          <CheckBox x:Name="SyncEnabled" Content="Synchroniser avant et après chaque partie" FontWeight="SemiBold" Margin="0,0,0,8"/>
          <TextBlock Foreground="#667085" TextWrapping="Wrap" Text="Si le cloud est plus récent, la version locale est remplacée automatiquement, avec une copie de secours. Si les deux ont changé, l'outil affiche les dates et te laisse choisir."/>
        </StackPanel></Border>
        <Border Style="{StaticResource Card}"><StackPanel>
          <RadioButton x:Name="ProvFolder" Content="Dossier synchronisé (le plus simple, sans API)" GroupName="Prov" FontWeight="SemiBold" Margin="0,0,0,4"/>
          <TextBlock Foreground="#667085" TextWrapping="Wrap" Text="Dossier de Google Drive pour ordinateur, OneDrive, Dropbox ou NAS." Margin="20,0,0,4"/>
          <DockPanel Margin="20,0,0,12"><Button x:Name="BtnBrowse" DockPanel.Dock="Right" Style="{StaticResource Secondary}" Content="Parcourir" Margin="8,0,0,0"/><TextBox x:Name="FolderPath" Padding="6,5"/></DockPanel>
          <RadioButton x:Name="ProvGoogle" Content="API Google Drive" GroupName="Prov" FontWeight="SemiBold" Margin="0,0,0,4"/>
          <StackPanel Margin="20,0,0,0">
            <TextBlock Foreground="#667085" TextWrapping="Wrap" Margin="0,0,0,6">1. Sur console.cloud.google.com, crée un projet et active Google Drive API.<LineBreak/>2. Écran de consentement : type Externe, puis publie l'application pour que la connexion n'expire pas après 7 jours.<LineBreak/>3. Identifiants : crée un client OAuth de type Application de bureau, puis colle ici son identifiant et son secret.</TextBlock>
            <TextBlock Style="{StaticResource Label}" Text="Identifiant client"/><TextBox x:Name="GoogleId" Style="{StaticResource Field}"/>
            <TextBlock Style="{StaticResource Label}" Text="Secret client (chiffré pour ce compte Windows)"/><PasswordBox x:Name="GoogleSecret" Padding="6,5" Margin="0,3,0,10"/>
            <StackPanel Orientation="Horizontal"><Button x:Name="BtnGoogle" Style="{StaticResource Secondary}" Content="Connecter mon compte Google"/><TextBlock x:Name="GoogleState" VerticalAlignment="Center" Margin="12,0,0,0" Foreground="#667085"/></StackPanel>
          </StackPanel>
        </StackPanel></Border>
        <Border Style="{StaticResource Card}"><StackPanel Orientation="Horizontal">
          <TextBlock VerticalAlignment="Center" Text="Copies de secours à garder :"/><TextBox x:Name="KeepBackups" Width="50" Padding="6,5" Margin="10,0,16,0"/>
          <Button x:Name="BtnCloudSave" Style="{StaticResource Primary}" Content="Enregistrer"/>
          <Button x:Name="BtnSyncNow" Style="{StaticResource Secondary}" Content="Synchroniser maintenant" Margin="8,0,0,0"/>
        </StackPanel></Border>
      </StackPanel></ScrollViewer>
      <ScrollViewer x:Name="PageSettings" Visibility="Collapsed" VerticalScrollBarVisibility="Auto"><StackPanel>
        <TextBlock Style="{StaticResource H1}" Text="Réglages"/>
        <TextBlock Style="{StaticResource Sub}" Text="Où vivent tes sauvegardes, ta configuration et tes clés. Les jeux restent dans Roms."/>
        <Border Style="{StaticResource Card}"><StackPanel>
          <RadioButton x:Name="DataPortable" Content="Portable : à côté de l'outil (rien sur C:)" GroupName="Data" FontWeight="SemiBold" Margin="0,0,0,6"/>
          <RadioButton x:Name="DataLocal" Content="Installé : sur ce PC, dans AppData" GroupName="Data" FontWeight="SemiBold" Margin="0,0,0,6"/>
          <RadioButton x:Name="DataCustom" Content="Autre dossier :" GroupName="Data" FontWeight="SemiBold" Margin="0,0,0,4"/>
          <DockPanel Margin="20,0,0,10"><Button x:Name="BtnDataBrowse" DockPanel.Dock="Right" Style="{StaticResource Secondary}" Content="Parcourir" Margin="8,0,0,0"/><TextBox x:Name="DataPath" Padding="6,5"/></DockPanel>
          <TextBlock x:Name="DataNow" Foreground="#667085" TextWrapping="Wrap" Margin="0,0,0,10"/>
          <Button x:Name="BtnDataApply" Style="{StaticResource Primary}" Content="Déplacer les données" HorizontalAlignment="Left"/>
        </StackPanel></Border>
        <Border Style="{StaticResource Card}"><StackPanel>
          <TextBlock Style="{StaticResource Label}" Text="Émulateur par défaut des nouveaux jeux"/>
          <ComboBox x:Name="DefaultEmu" Width="220" HorizontalAlignment="Left" Margin="0,5,0,10" Padding="6,5"/>
          <Button x:Name="BtnOpen" Style="{StaticResource Secondary}" Content="Ouvrir le dossier Files" HorizontalAlignment="Left"/>
        </StackPanel></Border>
        <Border Style="{StaticResource Card}"><StackPanel>
          <TextBlock Style="{StaticResource Label}" Text="Clé API SteamGridDB (images des jeux)"/>
          <TextBlock Foreground="#667085" TextWrapping="Wrap" Margin="0,2,0,6" Text="Gratuite : connecte-toi sur steamgriddb.com, puis Preferences, API. Chiffrée pour ce compte Windows."/>
          <PasswordBox x:Name="GridKey" Padding="6,5" Margin="0,0,0,8"/>
          <StackPanel Orientation="Horizontal"><Button x:Name="BtnGridKey" Style="{StaticResource Primary}" Content="Enregistrer la clé"/><TextBlock x:Name="GridKeyState" VerticalAlignment="Center" Margin="12,0,0,0" Foreground="#667085"/></StackPanel>
        </StackPanel></Border>
      </StackPanel></ScrollViewer>
    </Grid>
    <Border Grid.Column="1" Grid.Row="1" Background="#E9EBF0" Padding="28,7"><TextBlock x:Name="StatusBar" Foreground="#475067" TextTrimming="CharacterEllipsis"/></Border>
  </Grid>
</Window>
'@

$window = [Windows.Markup.XamlReader]::Load((New-Object Xml.XmlNodeReader $xaml))
$ui = @{}
foreach ($node in $xaml.SelectNodes("//*[@*[local-name()='Name']]")) { $n = $node.Attributes['x:Name'].Value; $ui[$n] = $window.FindName($n) }
$primary = $window.FindResource('Primary'); $secondary = $window.FindResource('Secondary'); $card = $window.FindResource('Card')

$script:Work = Join-Path $PSScriptRoot '_ui'
New-Item -ItemType Directory -Force $script:Work | Out-Null
$script:Busy = $null
$script:EmuNames = Get-EmulatorNames
$script:Settings = Get-Settings

function Reset-EmulatorNames {
    Import-EmulatorList
    $script:EmuNames = Get-EmulatorNames
    $ui.DefaultEmu.Items.Clear()
    Update-Games; Update-SettingsView
}

function New-Cover([string]$file) {
    $border = New-Object Windows.Controls.Border; $border.Width = 44; $border.Height = 66; $border.CornerRadius = '5'; $border.Margin = '0,0,14,0'; $border.Background = '#E3E6EC'; $border.ClipToBounds = $true
    $art = Find-Art $file 'grid'
    if ($art) {
        try {
            $bmp = New-Object Windows.Media.Imaging.BitmapImage
            $bmp.BeginInit(); $bmp.CacheOption = 'OnLoad'; $bmp.DecodePixelWidth = 120; $bmp.UriSource = New-Object Uri($art); $bmp.EndInit()
            $brush = New-Object Windows.Media.ImageBrush $bmp; $brush.Stretch = 'UniformToFill'
            $border.Background = $brush
        } catch { }
    }
    return $border
}

function Set-Status([string]$text) { $ui.StatusBar.Text = $text }

# Sans console, mais les fenêtres du processus (conflit, messages) restent visibles.
function Start-Background([string]$code) {
    $psi = New-Object Diagnostics.ProcessStartInfo
    $psi.FileName = 'powershell.exe'
    $psi.Arguments = '-NoProfile -ExecutionPolicy Bypass -STA -EncodedCommand ' + [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($code))
    $psi.UseShellExecute = $false; $psi.CreateNoWindow = $true
    return [Diagnostics.Process]::Start($psi)
}
function Show-Info([string]$text, [string]$icon = 'Information') { [void][Windows.MessageBox]::Show($window, $text, 'Switch 2 Manager', 'OK', $icon) }

# --- Tâches en arrière-plan ---
$timer = New-Object Windows.Threading.DispatcherTimer
$timer.Interval = [TimeSpan]::FromMilliseconds(400)
$timer.Add_Tick({
    $b = $script:Busy
    if (-not $b) { $timer.Stop(); return }
    if ($b.Progress) { & $b.Progress }
    if (-not $b.Process.HasExited) { return }
    $timer.Stop(); $script:Busy = $null; $ui.Pages.IsEnabled = $true
    try {
        $res = Get-Content -LiteralPath $b.Result -Raw -Encoding UTF8 | ConvertFrom-Json
        Remove-Item -LiteralPath $b.Result -Force
        & $b.Done $res
    } catch { Set-Status "Erreur : $($_.Exception.Message)" }
})

function Start-Worker([string]$action, [string]$arg, [string]$text, [scriptblock]$done, [scriptblock]$progress = $null) {
    if ($script:Busy) { return }
    $result = Join-Path $script:Work "$action.json"
    if (Test-Path -LiteralPath $result) { Remove-Item -LiteralPath $result -Force }
    $p = Start-Background "& '$((Join-Path $PSScriptRoot 'Worker.ps1').Replace("'", "''"))' -Action '$action' -Arg '$($arg.Replace("'", "''"))' -Result '$($result.Replace("'", "''"))'"
    $script:Busy = @{ Process = $p; Result = $result; Done = $done; Progress = $progress }
    $ui.Pages.IsEnabled = $false
    Set-Status $text
    $timer.Start()
}

# --- Jeux ---
function Update-Setup {
    $s = Get-SetupStatus
    $parts = @()
    $parts += if ($s.Keys) { 'Clés : OK' } else { 'Clés : manquantes' }
    $parts += if ($s.Firmware) { 'Firmware : OK' } else { 'Firmware : manquant' }
    $parts += "Émulateurs : $($s.Emulators)/$($script:Emus.Count)"
    $parts += "Jeux : $($s.Games)"
    $ui.SetupText.Text = $parts -join '     '
    $missing = (-not $s.Keys -and $s.KeysSource) -or (-not $s.Firmware -and $s.FirmwareSource)
    $ui.BtnPrereq.Visibility = if ($missing) { 'Visible' } else { 'Collapsed' }
}

function Update-Games {
    $ui.GamesList.Children.Clear()
    $roms = Get-RomFiles
    if (-not $roms.Count) {
        $t = New-Object Windows.Controls.TextBlock; $t.Text = 'Aucun jeu : place tes .nup, .nsp ou .xci dans Files\Roms.'; $t.Foreground = '#667085'
        [void]$ui.GamesList.Children.Add($t); return
    }
    foreach ($file in $roms) {
        $b = New-Object Windows.Controls.Border; $b.Style = $card
        $dock = New-Object Windows.Controls.DockPanel
        $play = New-Object Windows.Controls.Button; $play.Style = $primary; $play.Content = 'Jouer'; $play.Tag = $file; $play.Margin = '10,0,0,0'
        [Windows.Controls.DockPanel]::SetDock($play, 'Right')
        $play.Add_Click({
            $game = "Roms\$($this.Tag)"
            [void](Start-Background "& '$((Join-Path $script:Root 'Launcher.ps1').Replace("'", "''"))' -Game '$($game.Replace("'", "''"))'")
            Set-Status "Lancement de $(Get-DisplayName $this.Tag)..."
        })
        $combo = New-Object Windows.Controls.ComboBox; $combo.Width = 150; $combo.Tag = $file; $combo.Padding = '6,5'
        foreach ($e in $script:EmuNames) { [void]$combo.Items.Add($e) }
        $combo.SelectedItem = Get-GameEmulator $script:Settings $file
        [Windows.Controls.DockPanel]::SetDock($combo, 'Right')
        $combo.Add_SelectionChanged({
            $script:Settings = Get-Settings
            $script:Settings.games | Add-Member -NotePropertyName $this.Tag -NotePropertyValue ([string]$this.SelectedItem) -Force
            Save-Settings $script:Settings
            Set-Status "$(Get-DisplayName $this.Tag) utilisera $($this.SelectedItem)."
        })
        $name = New-Object Windows.Controls.TextBlock; $name.Text = Get-DisplayName $file; $name.FontSize = 16; $name.FontWeight = 'SemiBold'; $name.VerticalAlignment = 'Center'; $name.TextTrimming = 'CharacterEllipsis'
        $play.VerticalAlignment = 'Center'; $combo.VerticalAlignment = 'Center'
        $cover = New-Cover $file
        [Windows.Controls.DockPanel]::SetDock($cover, 'Left')
        [void]$dock.Children.Add($cover); [void]$dock.Children.Add($play); [void]$dock.Children.Add($combo); [void]$dock.Children.Add($name)
        $b.Child = $dock
        [void]$ui.GamesList.Children.Add($b)
    }
}

# --- Émulateurs ---
function Show-Emulators($rows) {
    $ui.EmusList.Children.Clear()
    foreach ($r in $rows) {
        $b = New-Object Windows.Controls.Border; $b.Style = $card
        $dock = New-Object Windows.Controls.DockPanel
        $notes = New-Object Windows.Controls.Button; $notes.Style = $secondary; $notes.Content = 'Notes'; $notes.Tag = $r.Page; $notes.Margin = '8,0,0,0'; $notes.VerticalAlignment = 'Center'
        [Windows.Controls.DockPanel]::SetDock($notes, 'Right')
        $notes.Add_Click({ if ($this.Tag -match '^https://') { Start-Process $this.Tag } })
        if ($r.Local) { $notes.Visibility = 'Collapsed' }
        [void]$dock.Children.Add($notes)
        if ($r.User) {
            $rm = New-Object Windows.Controls.Button; $rm.Style = $secondary; $rm.Content = 'Retirer'; $rm.Tag = $r.Name; $rm.Margin = '8,0,0,0'; $rm.VerticalAlignment = 'Center'
            [Windows.Controls.DockPanel]::SetDock($rm, 'Right')
            $rm.Add_Click({
                $msg = Remove-UserEmulator $this.Tag
                Set-Status $msg; Reset-EmulatorNames; Update-EmuCheck
            })
            [void]$dock.Children.Add($rm)
        }
        $needs = (-not $r.Local) -and (-not $r.UpToDate) -and (-not $r.Error) -and $r.Latest
        if ($needs) {
            $act = New-Object Windows.Controls.Button; $act.Style = $primary; $act.Tag = "$($r.Name)|$($r.Size)|$($r.Latest)"; $act.VerticalAlignment = 'Center'; $act.Margin = '8,0,0,0'
            $act.Content = if ($r.Present) { 'Mettre à jour' } else { 'Installer' }
            [Windows.Controls.DockPanel]::SetDock($act, 'Right')
            $act.Add_Click({
                $p = $this.Tag -split '\|'
                $q = "Installer $($p[0]) $($p[2]) ({0:N0} Mo) ?`n`nL'ancienne version est gardée dans Old Versions. Une version plus récente peut parfois moins bien faire tourner un jeu." -f ([long]$p[1] / 1MB)
                if ([Windows.MessageBox]::Show($window, $q, 'Switch 2 Manager', 'YesNo', 'Question') -ne 'Yes') { return }
                $script:InstallName = $p[0]; $script:InstallSize = [long]$p[1]
                Start-Worker 'install' $p[0] "$($p[0]) : téléchargement..." {
                    param($res)
                    Set-Status $res.Message
                    Show-Info $res.Message $(if ($res.Ok) { 'Information' } else { 'Warning' })
                    Update-Setup; Update-Steam; Update-EmuCheck
                } {
                    $zip = Join-Path $script:Root "_update\$($script:InstallName)\download.zip"
                    if ((Test-Path -LiteralPath $zip) -and $script:InstallSize -gt 0) {
                        $d = (Get-Item -LiteralPath $zip).Length
                        if ($d -lt $script:InstallSize) { Set-Status ("$($script:InstallName) : téléchargement {0:N0} %" -f (100 * $d / $script:InstallSize)) }
                        else { Set-Status "$($script:InstallName) : vérification et installation..." }
                    }
                }
            })
            [void]$dock.Children.Add($act)
        }
        $col = New-Object Windows.Controls.StackPanel
        $title = New-Object Windows.Controls.TextBlock; $title.Text = $r.Name; $title.FontSize = 16; $title.FontWeight = 'SemiBold'
        $about = New-Object Windows.Controls.TextBlock; $about.Foreground = '#667085'
        $installed = if ($r.Present -and $r.Installed) { $r.Installed } elseif ($r.Present) { 'version inconnue' } else { 'non installé' }
        $about.Text = "$($r.About)  ·  Installée : $installed" + $(if ($r.Local) { '' } elseif ($r.Latest) { "  ·  Disponible : $($r.Latest) ($($r.Date))" } else { '  ·  Disponible : ?' })
        $about.TextWrapping = 'Wrap'
        $state = New-Object Windows.Controls.TextBlock; $state.FontWeight = 'SemiBold'; $state.Margin = '0,3,0,0'
        if ($r.Local) { $state.Text = 'Copie locale : mise à jour manuelle'; $state.Foreground = '#667085' }
        elseif ($r.Error) { $state.Text = 'Vérification impossible'; $state.Foreground = '#C62828' }
        elseif ($r.UpToDate) { $state.Text = 'À jour'; $state.Foreground = '#1B8A3A' }
        elseif ($r.Present) { $state.Text = 'Mise à jour disponible'; $state.Foreground = '#C77700' }
        else { $state.Text = 'Pas encore installé'; $state.Foreground = '#C77700' }
        [void]$col.Children.Add($title); [void]$col.Children.Add($about); [void]$col.Children.Add($state)
        [void]$dock.Children.Add($col)
        $b.Child = $dock
        [void]$ui.EmusList.Children.Add($b)
    }
}

function Update-EmuCheck {
    Start-Worker 'status' '' 'Vérification des versions...' {
        param($res)
        if ($res.Ok) { Show-Emulators @($res.Rows); Set-Status ('Versions vérifiées à ' + (Get-Date -Format 'HH:mm') + '.') }
        else { Set-Status "Vérification impossible : $($res.Message)" }
    }
}

# --- Steam ---
function Update-Steam {
    $ui.SteamGames.Children.Clear()
    $script:SteamPathNow = Get-SteamPath
    $ui.SteamAccount.Items.Clear()
    if (-not $script:SteamPathNow) { $ui.SteamInfo.Text = 'Steam introuvable sur ce PC.'; $ui.BtnSteamSave.IsEnabled = $false; return }
    $script:SteamAccounts = @(Get-SteamAccounts $script:SteamPathNow)
    foreach ($a in $script:SteamAccounts) { [void]$ui.SteamAccount.Items.Add($a.Label) }
    if ($script:SteamAccounts.Count) { $ui.SteamAccount.SelectedIndex = 0 } else { $ui.SteamInfo.Text = 'Aucun compte Steam trouvé.'; $ui.BtnSteamSave.IsEnabled = $false }
}

function Update-SteamGames {
    $ui.SteamGames.Children.Clear()
    if ($ui.SteamAccount.SelectedIndex -lt 0) { return }
    $path = Get-SteamVdfPath $script:SteamPathNow $script:SteamAccounts[$ui.SteamAccount.SelectedIndex].Id
    try {
        $entries = Read-Shortcuts $path
        $managed = @($entries | ForEach-Object { Get-ManagedGame $_ } | Where-Object { $_ })
        foreach ($file in Get-RomFiles) {
            $c = New-Object Windows.Controls.CheckBox; $c.Content = Get-DisplayName $file; $c.Tag = $file; $c.Margin = '0,3,0,3'
            $c.IsChecked = ($managed -contains "Roms\$file")
            [void]$ui.SteamGames.Children.Add($c)
        }
        $marker = Get-SteamMarker
        $stale = @($entries | Where-Object { (Get-ManagedGame $_) -and -not ([string](Get-Field $_ 'LaunchOptions')).Contains($marker) })
        if ($stale.Count) { $ui.SteamInfo.Text = "$($stale.Count) raccourci(s) pointent vers un ancien emplacement : clique sur Enregistrer pour les réparer."; $ui.SteamInfo.Foreground = '#C77700' }
        else { $ui.SteamInfo.Text = "Steam sera fermé puis rouvert si nécessaire : quitte d'abord un jeu Steam en cours."; $ui.SteamInfo.Foreground = '#667085' }
        $ui.BtnSteamSave.IsEnabled = $true
    } catch {
        $ui.SteamInfo.Text = "Bibliothèque non modifiable : $($_.Exception.Message)"; $ui.SteamInfo.Foreground = '#C62828'; $ui.BtnSteamSave.IsEnabled = $false
    }
}

# --- Cloud ---
function Update-CloudView {
    $s = (Get-Settings).sync
    $ui.SyncEnabled.IsChecked = [bool]$s.enabled
    $ui.ProvFolder.IsChecked = ($s.provider -ne 'google'); $ui.ProvGoogle.IsChecked = ($s.provider -eq 'google')
    $ui.FolderPath.Text = $s.folderPath; $ui.GoogleId.Text = $s.googleClientId; $ui.KeepBackups.Text = [string]$s.keepBackups
    $ui.GoogleSecret.Password = ''
    $has = [bool](Unprotect-Text $s.googleClientSecret)
    if ($s.googleClientSecret -and -not $has) { $ui.GoogleState.Text = 'Secret à ressaisir sur ce PC.' }
    elseif (Unprotect-Text $s.googleRefreshToken) { $ui.GoogleState.Text = 'Compte connecté.' }
    elseif ($has) { $ui.GoogleState.Text = 'Secret enregistré, compte non connecté.' }
    else { $ui.GoogleState.Text = 'Non connecté.' }
}

function Save-CloudView {
    $settings = Get-Settings
    $keep = 0
    if (-not [int]::TryParse($ui.KeepBackups.Text, [ref]$keep) -or $keep -lt 1) { throw 'Le nombre de copies de secours doit être au moins 1.' }
    $settings.sync.enabled = [bool]$ui.SyncEnabled.IsChecked
    $settings.sync.provider = if ($ui.ProvGoogle.IsChecked) { 'google' } else { 'folder' }
    $settings.sync.folderPath = $ui.FolderPath.Text.Trim()
    $settings.sync.googleClientId = $ui.GoogleId.Text.Trim()
    $settings.sync.keepBackups = $keep
    if ($ui.GoogleSecret.Password) { $settings.sync.googleClientSecret = Protect-Text $ui.GoogleSecret.Password }
    if ($settings.sync.enabled -and $settings.sync.provider -eq 'folder' -and -not (Test-Path -LiteralPath $settings.sync.folderPath)) { throw 'Choisis un dossier de synchronisation existant.' }
    Save-Settings $settings
    $script:Settings = $settings
    Update-CloudView
}

# --- Réglages ---
function Update-SettingsView {
    $s = Get-Settings
    $ui.DataPortable.IsChecked = ($s.dataLocation -eq 'portable'); $ui.DataLocal.IsChecked = ($s.dataLocation -eq 'local'); $ui.DataCustom.IsChecked = ($s.dataLocation -eq 'custom')
    $ui.DataPath.Text = $s.customDataPath
    $ui.DataNow.Text = "Emplacement actuel : $(Get-DataPathFor $s)"
    if (-not $ui.DefaultEmu.Items.Count) { foreach ($e in $script:EmuNames) { [void]$ui.DefaultEmu.Items.Add($e) } }
    $ui.DefaultEmu.SelectedItem = $s.defaultEmulator
    $ui.GridKey.Password = ''
    $ui.GridKeyState.Text = if (Unprotect-Text $s.steamGridKey) { 'Clé enregistrée.' } elseif ($s.steamGridKey) { 'Clé à ressaisir sur ce PC.' } else { 'Aucune clé.' }
}

# --- Premier lancement ---
function Update-Welcome {
    $s = Get-Settings
    $found = @(Find-ExistingInstalls)
    $ui.WelcomeFound.Children.Clear()
    if ($found.Count) {
        $ui.WelcomeFoundText.Text = "Trouvé dans AppData. Importer copie les données sans modifier la source ni écraser ce qui existe déjà."
        foreach ($f in $found) {
            $dock = New-Object Windows.Controls.DockPanel; $dock.Margin = '0,4,0,4'
            $btn = New-Object Windows.Controls.Button; $btn.Style = $secondary; $btn.Content = 'Importer'; $btn.Tag = $f.Path
            [Windows.Controls.DockPanel]::SetDock($btn, 'Right')
            $btn.Add_Click({
                Start-Worker 'import' $this.Tag 'Import en cours...' { param($res) $ui.WelcomeMessage.Text = $res.Message; Set-Status $res.Message; Update-Welcome; Update-Setup }
            })
            $t = New-Object Windows.Controls.TextBlock; $t.TextWrapping = 'Wrap'; $t.VerticalAlignment = 'Center'
            $bits = @(); if ($f.Keys) { $bits += 'clés' }; if ($f.Config) { $bits += 'configuration' }; $bits += "$($f.Saves) fichier(s) de sauvegarde"
            $t.Text = "$($f.Name) : " + ($bits -join ', ') + $(if ($f.Compatible) { '' } else { ' (ancien émulateur : clés et sauvegardes seulement)' })
            [void]$dock.Children.Add($btn); [void]$dock.Children.Add($t)
            [void]$ui.WelcomeFound.Children.Add($dock)
        }
    } else { $ui.WelcomeFoundText.Text = "Rien d'autre trouvé sur ce PC." }
    if (-not $ui.WelcomePortable.IsChecked -and -not $ui.WelcomeLocal.IsChecked) { $ui.WelcomePortable.IsChecked = ($s.dataLocation -ne 'local'); $ui.WelcomeLocal.IsChecked = ($s.dataLocation -eq 'local') }
    $ui.WelcomeLocationNote.Text = "Emplacement actuel : $(Get-DataPathFor $s)"
    $st = Get-SetupStatus
    $line = @()
    $line += if ($st.Keys) { 'Clés : OK' } elseif ($st.KeysSource) { 'Clés : à installer depuis Files' } else { 'Clés : introuvables, place prod.keys dans un dossier de Files' }
    $line += if ($st.Firmware) { 'Firmware : OK' } elseif ($st.FirmwareSource) { 'Firmware : à installer depuis Files' } else { 'Firmware : introuvable, place un Firmware*.zip dans Files' }
    $line += "Jeux : $($st.Games)" + $(if ($st.Games) { '' } else { ', place-les dans Files\Roms' })
    $line += "Émulateurs : $($st.Emulators)"
    $ui.WelcomeStatus.Text = $line -join "`n"
    $ui.WelcomePrereq.Visibility = if ((-not $st.Keys -and $st.KeysSource) -or (-not $st.Firmware -and $st.FirmwareSource)) { 'Visible' } else { 'Collapsed' }
    $ui.WelcomeInstall.Visibility = if (Find-Exe (Join-Path $script:Root 'Lichii')) { 'Collapsed' } else { 'Visible' }
}

function Complete-Welcome {
    $s = Get-Settings; $s.setupDone = $true; Save-Settings $s; $script:Settings = $s
    $ui.Welcome.Visibility = 'Collapsed'
    Update-Setup; Update-SettingsView; Update-Steam
}

function Select-Folder([string]$start) {
    $d = New-Object Windows.Forms.FolderBrowserDialog
    if ($start -and (Test-Path -LiteralPath $start)) { $d.SelectedPath = $start }
    if ($d.ShowDialog() -eq 'OK') { return $d.SelectedPath }
    return $null
}

# --- Liens de l'interface ---
$pages = @{ NavGames = 'PageGames'; NavEmus = 'PageEmus'; NavSteam = 'PageSteam'; NavCloud = 'PageCloud'; NavSettings = 'PageSettings' }
foreach ($k in $pages.Keys) {
    $ui[$k].Add_Checked({
        foreach ($v in $pages.Values) { $ui[$v].Visibility = 'Collapsed' }
        $ui[$pages[$this.Name]].Visibility = 'Visible'
    })
}
$ui.BtnPrereq.Add_Click({ Start-Worker 'prereq' '' 'Installation des clés et du firmware...' { param($res) Set-Status $res.Message; Update-Setup } })
$ui.BtnRefresh.Add_Click({ Update-EmuCheck })
$ui.SteamAccount.Add_SelectionChanged({ Update-SteamGames })
$ui.BtnSteamSave.Add_Click({
    $games = @($ui.SteamGames.Children | Where-Object { $_.IsChecked } | ForEach-Object { [string]$_.Tag })
    if (Get-Process steam -ErrorAction SilentlyContinue) {
        $q = "Steam doit être fermé pour enregistrer, sinon il écraserait le changement.`n`nFermer Steam maintenant ? Il sera rouvert ensuite."
        if ([Windows.MessageBox]::Show($window, $q, 'Switch 2 Manager', 'YesNo', 'Question') -ne 'Yes') { return }
    }
    $arg = @{ account = $script:SteamAccounts[$ui.SteamAccount.SelectedIndex].Id; games = $games } | ConvertTo-Json -Compress
    Start-Worker 'steam' $arg 'Enregistrement dans Steam...' { param($res) Set-Status $res.Message; if (-not $res.Ok) { Show-Info $res.Message 'Warning' }; Update-SteamGames }
})
$ui.BtnBrowse.Add_Click({ $p = Select-Folder $ui.FolderPath.Text; if ($p) { $ui.FolderPath.Text = $p } })
$ui.BtnCloudSave.Add_Click({ try { Save-CloudView; Set-Status 'Réglages du cloud enregistrés.' } catch { Show-Info $_.Exception.Message 'Warning' } })
$ui.BtnGoogle.Add_Click({
    try { Save-CloudView } catch { Show-Info $_.Exception.Message 'Warning'; return }
    Start-Worker 'google' '' 'Connexion : termine dans ton navigateur...' { param($res) Set-Status $res.Message; if (-not $res.Ok) { Show-Info $res.Message 'Warning' }; Update-CloudView }
})
$ui.BtnSyncNow.Add_Click({
    try { Save-CloudView } catch { Show-Info $_.Exception.Message 'Warning'; return }
    Start-Worker 'sync' '' 'Synchronisation...' { param($res) Set-Status $res.Message; if (-not $res.Ok) { Show-Info $res.Message 'Warning' } }
})
$ui.BtnDataBrowse.Add_Click({ $p = Select-Folder $ui.DataPath.Text; if ($p) { $ui.DataPath.Text = $p; $ui.DataCustom.IsChecked = $true } })
$ui.BtnDataApply.Add_Click({
    $loc = if ($ui.DataLocal.IsChecked) { 'local' } elseif ($ui.DataCustom.IsChecked) { 'custom' } else { 'portable' }
    if ($loc -eq 'custom' -and -not $ui.DataPath.Text.Trim()) { Show-Info 'Choisis un dossier.' 'Warning'; return }
    $q = 'Copier les données vers ce nouvel emplacement ? L''ancienne copie est gardée jusqu''à ce que tu la supprimes.'
    if ([Windows.MessageBox]::Show($window, $q, 'Switch 2 Manager', 'YesNo', 'Question') -ne 'Yes') { return }
    Start-Worker 'migrate' "$loc|$($ui.DataPath.Text.Trim())" 'Déplacement des données...' { param($res) Set-Status $res.Message; Show-Info $res.Message $(if ($res.Ok) { 'Information' } else { 'Warning' }); Update-SettingsView; Update-Setup }
})
$ui.DefaultEmu.Add_SelectionChanged({
    if (-not $ui.DefaultEmu.SelectedItem) { return }
    $s = Get-Settings
    if ($s.defaultEmulator -ne [string]$ui.DefaultEmu.SelectedItem) { $s.defaultEmulator = [string]$ui.DefaultEmu.SelectedItem; Save-Settings $s; $script:Settings = $s; Update-Games }
})
$ui.BtnOpen.Add_Click({ Start-Process explorer.exe "`"$script:Root`"" })
$ui.BtnGridKey.Add_Click({
    if (-not $ui.GridKey.Password) { return }
    $s = Get-Settings; $s.steamGridKey = Protect-Text $ui.GridKey.Password; Save-Settings $s; Update-SettingsView; Set-Status 'Clé SteamGridDB enregistrée.'
})
$ui.BtnArt.Add_Click({
    Start-Worker 'art' '' 'Récupération des images...' { param($res) Set-Status $res.Message; if (-not $res.Ok) { Show-Info $res.Message 'Warning' }; Update-Games }
})
$ui.BtnAddGitHub.Add_Click({
    $arg = @{ name = $ui.AddName.Text.Trim(); repo = $ui.AddRepo.Text.Trim(); asset = $ui.AddAsset.Text.Trim() } | ConvertTo-Json -Compress
    Start-Worker 'addgithub' $arg 'Vérification du dépôt...' {
        param($res) Set-Status $res.Message
        if ($res.Ok) { $ui.AddName.Text = ''; $ui.AddRepo.Text = ''; $ui.AddAsset.Text = ''; Reset-EmulatorNames; Update-EmuCheck } else { Show-Info $res.Message 'Warning' }
    }
})
$ui.BtnAddLocal.Add_Click({
    if (-not $ui.AddName.Text.Trim()) { Show-Info 'Donne d''abord un nom à cet émulateur.' 'Warning'; return }
    $d = New-Object Windows.Forms.OpenFileDialog; $d.Filter = 'Programme (*.exe)|*.exe'
    if ($d.ShowDialog() -ne 'OK') { return }
    $arg = @{ name = $ui.AddName.Text.Trim(); exe = $d.FileName } | ConvertTo-Json -Compress
    Start-Worker 'addlocal' $arg 'Copie de l''émulateur...' {
        param($res) Set-Status $res.Message
        if ($res.Ok) { $ui.AddName.Text = ''; Reset-EmulatorNames; Update-Setup; Update-EmuCheck } else { Show-Info $res.Message 'Warning' }
    }
})
$ui.WelcomePrereq.Add_Click({ Start-Worker 'prereq' '' 'Installation des clés et du firmware...' { param($res) $ui.WelcomeMessage.Text = $res.Message; Update-Welcome; Update-Setup } })
$ui.WelcomeInstall.Add_Click({
    $ui.WelcomeMessage.Text = 'Téléchargement de Lichii...'
    $ui.Welcome.IsEnabled = $false
    Start-Worker 'install' 'Lichii' 'Installation de Lichii...' { param($res) $ui.Welcome.IsEnabled = $true; $ui.WelcomeMessage.Text = $res.Message; Update-Welcome; Update-Setup; Update-Steam; Update-EmuCheck }
})
$ui.WelcomeDone.Add_Click({
    $want = if ($ui.WelcomeLocal.IsChecked) { 'local' } else { 'portable' }
    if ($want -eq (Get-Settings).dataLocation) { Complete-Welcome; return }
    $ui.Welcome.IsEnabled = $false
    $ui.WelcomeMessage.Text = 'Déplacement des données...'
    Start-Worker 'migrate' "$want|" 'Déplacement des données...' {
        param($res)
        $ui.Welcome.IsEnabled = $true
        $ui.WelcomeMessage.Text = $res.Message
        if ($res.Ok) { Complete-Welcome; Show-Info $res.Message } else { Show-Info $res.Message 'Warning' }
    }
})

$window.Add_Loaded({
    try { [void](Repair-Setup) } catch { Show-Info $_.Exception.Message 'Warning' }
    Update-Setup; Update-Games; Update-Steam; Update-CloudView; Update-SettingsView; Update-EmuCheck
    if (-not (Get-Settings).setupDone) { Update-Welcome; $ui.Welcome.Visibility = 'Visible' }
})
$window.Add_Closed({ $timer.Stop(); if (Test-Path -LiteralPath $script:Work) { Remove-Item -LiteralPath $script:Work -Recurse -Force -ErrorAction SilentlyContinue } })
[void]$window.ShowDialog()

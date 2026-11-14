use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;

macro_rules! define_strings {
    ($($field:ident: $es:expr, $en:expr, $de:expr, $fr:expr, $it:expr, $zh:expr, $ja:expr, $ru:expr;)*) => {
        pub struct Strings {
            $(pub $field: &'static str,)*
        }

        const ES: Strings = Strings { $($field: $es,)* };
        const EN: Strings = Strings { $($field: $en,)* };
        const DE: Strings = Strings { $($field: $de,)* };
        const FR: Strings = Strings { $($field: $fr,)* };
        const IT: Strings = Strings { $($field: $it,)* };
        const ZH: Strings = Strings { $($field: $zh,)* };
        const JA: Strings = Strings { $($field: $ja,)* };
        const RU: Strings = Strings { $($field: $ru,)* };
    };
}

define_strings! {
    app_subtitle: "Control de actualizaciones automáticas de Windows", "Windows automatic update control", "Windows-Update automatisch steuern", "Contrôle des mises à jour automatiques de Windows", "Controllo degli aggiornamenti automatici di Windows", "Windows 自动更新控制", "Windows 自動更新の制御", "Управление автоматическими обновлениями Windows";
    session_elevated: "Sesión elevada", "Elevated session", "Erhöhte Sitzung", "Session élevée", "Sessione con privilegi", "已提升权限", "管理者セッション", "Повышенные права";
    session_standard: "Sesión estándar", "Standard session", "Standard-Sitzung", "Session standard", "Sessione standard", "标准权限", "標準セッション", "Стандартные права";
    language: "Idioma", "Language", "Sprache", "Langue", "Lingua", "语言", "言語", "Язык";
    check_interval: "Comprobación cada {n} s", "Checks every {n} s", "Prüfung alle {n} s", "Vérification toutes les {n} s", "Controllo ogni {n} s", "每 {n} 秒检查一次", "{n} 秒ごとに確認", "Проверка каждые {n} с";
    state_heading: "Estado del sistema", "System status", "Systemstatus", "État du système", "Stato del sistema", "系统状态", "システム状態", "Состояние системы";
    refresh_now: "Actualizar ahora", "Refresh now", "Jetzt aktualisieren", "Actualiser maintenant", "Aggiorna ora", "立即刷新", "今すぐ更新", "Обновить сейчас";
    start_with_windows: "Iniciar con Windows", "Start with Windows", "Mit Windows starten", "Démarrer avec Windows", "Avvia con Windows", "随 Windows 启动", "Windows と一緒に起動", "Запускать с Windows";
    start_with_windows_desc: "Abre PausaPatch en la bandeja al iniciar sesión", "Opens PausaPatch in the system tray at sign-in", "Öffnet PausaPatch beim Anmelden im Infobereich", "Ouvre PausaPatch dans la zone de notification à la connexion", "Apre PausaPatch nell'area di notifica all'accesso", "登录时在系统托盘中打开 PausaPatch", "サインイン時に通知領域で PausaPatch を開く", "Открывать PausaPatch в области уведомлений при входе";
    log: "Registro", "Log", "Protokoll", "Journal", "Registro", "日志", "ログ", "Журнал";
    check_now: "Comprobar ahora", "Check now", "Jetzt prüfen", "Vérifier maintenant", "Controlla ora", "立即检查", "今すぐ確認", "Проверить сейчас";
    watching: "Vigilando", "Watching", "Überwachung aktiv", "Surveillance active", "Monitoraggio attivo", "正在监控", "監視中", "Наблюдение";
    paused_title: "Actualizaciones pausadas", "Updates paused", "Updates pausiert", "Mises à jour suspendues", "Aggiornamenti sospesi", "更新已暂停", "更新は一時停止中", "Обновления приостановлены";
    paused_sub: "Windows Update está desactivado. El equipo no instalará parches automáticamente.", "Windows Update is disabled. The device will not install updates automatically.", "Windows Update ist deaktiviert. Das Gerät installiert keine Updates automatisch.", "Windows Update est désactivé. L'appareil n'installera pas de mises à jour automatiquement.", "Windows Update è disattivato. Il dispositivo non installerà aggiornamenti automaticamente.", "Windows Update 已禁用，设备将不会自动安装更新。", "Windows Update は無効です。デバイスは更新プログラムを自動インストールしません。", "Windows Update отключён. Устройство не будет автоматически устанавливать обновления.";
    active_title: "Actualizaciones activas", "Updates active", "Updates aktiv", "Mises à jour actives", "Aggiornamenti attivi", "更新处于活动状态", "更新は有効", "Обновления включены";
    active_sub: "Windows puede descargar e instalar parches y reiniciar el equipo sin avisar.", "Windows can download and install updates and restart the device without warning.", "Windows kann Updates herunterladen, installieren und das Gerät ohne Vorwarnung neu starten.", "Windows peut télécharger et installer des mises à jour et redémarrer l'appareil sans prévenir.", "Windows può scaricare e installare aggiornamenti e riavviare il dispositivo senza avvisare.", "Windows 可能会下载并安装更新，并在未提醒的情况下重启设备。", "Windows は更新プログラムをダウンロードしてインストールし、予告なく再起動する場合があります。", "Windows может скачивать и устанавливать обновления и перезагружать устройство без предупреждения.";
    mixed_title: "Protección incompleta", "Incomplete protection", "Schutz unvollständig", "Protection incomplète", "Protezione incompleta", "保护不完整", "保護が不完全です", "Защита неполная";
    mixed_sub: "Algunos componentes de Windows Update siguen activos.", "Some Windows Update components are still active.", "Einige Windows Update-Komponenten sind noch aktiv.", "Certains composants de Windows Update sont encore actifs.", "Alcuni componenti di Windows Update sono ancora attivi.", "部分 Windows Update 组件仍处于活动状态。", "一部の Windows Update コンポーネントがまだ有効です。", "Некоторые компоненты Windows Update всё ещё активны.";
    unknown_title: "Estado desconocido", "Status unknown", "Status unbekannt", "État inconnu", "Stato sconosciuto", "状态未知", "状態が不明です", "Состояние неизвестно";
    unknown_sub: "No se pudo leer el estado de Windows Update.", "Could not read the Windows Update status.", "Der Windows Update-Status konnte nicht gelesen werden.", "Impossible de lire l'état de Windows Update.", "Impossibile leggere lo stato di Windows Update.", "无法读取 Windows Update 状态。", "Windows Update の状態を読み取れませんでした。", "Не удалось прочитать состояние Windows Update.";
    protection_on: "Protección activa", "Protection on", "Schutz aktiv", "Protection active", "Protezione attiva", "保护已启用", "保護は有効", "Защита включена";
    protection_off: "Protección desactivada", "Protection off", "Schutz deaktiviert", "Protection désactivée", "Protezione disattivata", "保护已关闭", "保護は無効", "Защита отключена";
    protection_mixed: "Revisar protección", "Review protection", "Schutz prüfen", "Vérifier la protection", "Verifica protezione", "检查保护", "保護を確認", "Проверить защиту";
    paused_since: "Pausadas desde {date}", "Paused since {date}", "Pausiert seit {date}", "Suspendues depuis le {date}", "Sospesi dal {date}", "自 {date} 起暂停", "{date} から一時停止", "Приостановлено с {date}";
    reactivated_since: "Reactivadas {date} · cambio detectado", "Reactivated {date} · change detected", "Reaktiviert am {date} · Änderung erkannt", "Réactivées le {date} · changement détecté", "Riattivati il {date} · modifica rilevata", "{date} 重新启用 · 检测到更改", "{date} に再有効化 · 変更を検出", "Включено {date} · обнаружено изменение";
    mixed_since: "Estado mixto desde {date}", "Mixed state since {date}", "Gemischter Status seit {date}", "État mixte depuis le {date}", "Stato misto dal {date}", "自 {date} 起状态混合", "{date} から状態が混在", "Смешанное состояние с {date}";
    checked_at: "Comprobado {date}", "Checked {date}", "Geprüft am {date}", "Vérifié le {date}", "Verificato il {date}", "检查于 {date}", "{date} に確認", "Проверено {date}";
    service_wuauserv: "Servicio de Windows Update", "Windows Update service", "Windows Update-Dienst", "Service Windows Update", "Servizio Windows Update", "Windows Update 服务", "Windows Update サービス", "Служба Windows Update";
    service_uso: "Update Orchestrator", "Update Orchestrator", "Update Orchestrator", "Update Orchestrator", "Update Orchestrator", "更新协调器", "更新オーケストレーター", "Оркестратор обновлений";
    service_medic: "Windows Update Medic", "Windows Update Medic", "Windows Update Medic", "Windows Update Medic", "Windows Update Medic", "Windows Update 医疗服务", "Windows Update 医療サービス", "Служба лечения Windows Update";
    tasks: "Tareas programadas", "Scheduled tasks", "Geplante Aufgaben", "Tâches planifiées", "Attività pianificate", "计划任务", "スケジュールされたタスク", "Запланированные задачи";
    policy: "Política NoAutoUpdate", "NoAutoUpdate policy", "NoAutoUpdate-Richtlinie", "Stratégie NoAutoUpdate", "Criterio NoAutoUpdate", "NoAutoUpdate 策略", "NoAutoUpdate ポリシー", "Политика NoAutoUpdate";
    dll_renamed: "DLL renombrada a _BAK", "DLL renamed to _BAK", "DLL in _BAK umbenannt", "DLL renommée en _BAK", "DLL rinominata in _BAK", "DLL 已重命名为 _BAK", "DLL を _BAK に名前変更", "DLL переименована в _BAK";
    dll_restored: "DLL original restaurada", "Original DLL restored", "Original-DLL wiederhergestellt", "DLL d'origine restaurée", "DLL originale ripristinata", "已恢复原始 DLL", "元の DLL を復元", "Исходная DLL восстановлена";
    disabled: "Deshabilitado", "Disabled", "Deaktiviert", "Désactivé", "Disattivato", "已禁用", "無効", "Отключено";
    automatic: "Automático", "Automatic", "Automatisch", "Automatique", "Automatico", "自动", "自動", "Автоматически";
    manual: "Manual", "Manual", "Manuell", "Manuel", "Manuale", "手动", "手動", "Вручную";
    unconfigured: "Sin configurar", "Not configured", "Nicht konfiguriert", "Non configuré", "Non configurato", "未配置", "未構成", "Не настроено";
    enabled: "Activada", "Enabled", "Aktiviert", "Activée", "Attivata", "已启用", "有効", "Включена";
    policy_off: "Desactivada", "Off", "Deaktiviert", "Désactivée", "Disattivata", "已关闭", "無効", "Отключена";
    tasks_disabled: "{n} deshabilitadas", "{n} disabled", "{n} deaktiviert", "{n} désactivées", "{n} disattivate", "{n} 个已禁用", "{n} 件無効", "{n} отключено";
    tasks_enabled: "{n} habilitadas", "{n} enabled", "{n} aktiviert", "{n} activées", "{n} attivate", "{n} 个已启用", "{n} 件有効", "{n} включено";
    tasks_partial: "{enabled} habilitadas, {disabled} deshabilitadas", "{enabled} enabled, {disabled} disabled", "{enabled} aktiviert, {disabled} deaktiviert", "{enabled} activées, {disabled} désactivées", "{enabled} attivate, {disabled} disattivate", "{enabled} 个已启用，{disabled} 个已禁用", "{enabled} 件有効、{disabled} 件無効", "{enabled} включено, {disabled} отключено";
    tasks_partial_active: "{disabled} deshabilitadas, {enabled} habilitadas", "{disabled} disabled, {enabled} enabled", "{disabled} deaktiviert, {enabled} aktiviert", "{disabled} désactivées, {enabled} activées", "{disabled} disattivate, {enabled} attivate", "{disabled} 个已禁用，{enabled} 个已启用", "{disabled} 件無効、{enabled} 件有効", "{disabled} отключено, {enabled} включено";
    applying: "Aplicando cambios…", "Applying changes…", "Änderungen werden angewendet…", "Application des modifications…", "Applicazione delle modifiche…", "正在应用更改…", "変更を適用しています…", "Применение изменений…";
    operation_failed: "No se pudieron aplicar los cambios.", "Changes could not be applied.", "Änderungen konnten nicht angewendet werden.", "Impossible d'appliquer les modifications.", "Impossibile applicare le modifiche.", "无法应用更改。", "変更を適用できませんでした。", "Не удалось применить изменения.";
    check_failed: "No se pudo leer el estado de Windows Update.", "Could not read the Windows Update status.", "Der Windows Update-Status konnte nicht gelesen werden.", "Impossible de lire l'état de Windows Update.", "Impossibile leggere lo stato di Windows Update.", "无法读取 Windows Update 状态。", "Windows Update の状態を読み取れませんでした。", "Не удалось прочитать состояние Windows Update.";
    notice_change_title: "Se detectó un cambio", "Change detected", "Änderung erkannt", "Changement détecté", "Modifica rilevata", "检测到更改", "変更を検出しました", "Обнаружено изменение";
    notice_reactivated_title: "Actualizaciones reactivadas", "Updates reactivated", "Updates reaktiviert", "Mises à jour réactivées", "Aggiornamenti riattivati", "更新已重新启用", "更新が再有効化されました", "Обновления снова включены";
    notice_reactivated_body: "Windows Update volvió a activarse por su cuenta.", "Windows Update reactivated itself.", "Windows Update wurde eigenständig wieder aktiviert.", "Windows Update s'est réactivé de lui-même.", "Windows Update si è riattivato da solo.", "Windows Update 再次自行启用。", "Windows Update が自動的に再有効化されました。", "Windows Update снова включился самостоятельно.";
    notice_paused_title: "Actualizaciones pausadas", "Updates paused", "Updates pausiert", "Mises à jour suspendues", "Aggiornamenti sospesi", "更新已暂停", "更新が一時停止されました", "Обновления приостановлены";
    notice_paused_body: "Windows Update se pausó fuera de PausaPatch.", "Windows Update was paused outside PausaPatch.", "Windows Update wurde außerhalb von PausaPatch pausiert.", "Windows Update a été suspendu en dehors de PausaPatch.", "Windows Update è stato sospeso fuori da PausaPatch.", "Windows Update 在 PausaPatch 之外被暂停。", "Windows Update が PausaPatch 以外で一時停止されました。", "Windows Update был приостановлен вне PausaPatch.";
    notice_mixed_title: "Protección incompleta", "Incomplete protection", "Schutz unvollständig", "Protection incomplète", "Protezione incompleta", "保护不完整", "保護が不完全です", "Защита неполная";
    notice_mixed_body: "Algunos componentes de Windows Update cambiaron fuera de PausaPatch.", "Some Windows Update components changed outside PausaPatch.", "Einige Windows Update-Komponenten wurden außerhalb von PausaPatch geändert.", "Certains composants de Windows Update ont changé en dehors de PausaPatch.", "Alcuni componenti di Windows Update sono cambiati fuori da PausaPatch.", "部分 Windows Update 组件在 PausaPatch 之外被更改。", "一部の Windows Update コンポーネントが PausaPatch 以外で変更されました。", "Некоторые компоненты Windows Update были изменены вне PausaPatch.";
    notice_stays: "No se cierra automáticamente: requiere tu confirmación.", "Does not close automatically: your confirmation is required.", "Wird nicht automatisch geschlossen: Deine Bestätigung ist erforderlich.", "Ne se ferme pas automatiquement : votre confirmation est requise.", "Non si chiude automaticamente: serve la tua conferma.", "此通知不会自动关闭，需要你的确认。", "この通知は自動的に閉じません。確認が必要です。", "Не закрывается автоматически — требуется ваше подтверждение.";
    dismiss: "Ignorar", "Ignore", "Ignorieren", "Ignorer", "Ignora", "忽略", "無視", "Игнорировать";
    pause_again: "Volver a pausar", "Pause again", "Erneut pausieren", "Suspendre à nouveau", "Sospendi di nuovo", "重新暂停", "もう一度一時停止", "Приостановить снова";
    close: "Cerrar", "Close", "Schließen", "Fermer", "Chiudi", "关闭", "閉じる", "Закрыть";
    tray_show: "Mostrar PausaPatch", "Show PausaPatch", "PausaPatch anzeigen", "Afficher PausaPatch", "Mostra PausaPatch", "显示 PausaPatch", "PausaPatch を表示", "Показать PausaPatch";
    tray_pause: "Pausar actualizaciones", "Pause updates", "Updates pausieren", "Suspendre les mises à jour", "Sospendi aggiornamenti", "暂停更新", "更新を一時停止", "Приостановить обновления";
    tray_resume: "Reactivar actualizaciones", "Resume updates", "Updates reaktivieren", "Réactiver les mises à jour", "Riattiva aggiornamenti", "恢复更新", "更新を再開", "Возобновить обновления";
    tray_exit: "Salir", "Exit", "Beenden", "Quitter", "Esci", "退出", "終了", "Выход";
    admin_required: "PausaPatch necesita ejecutarse como administrador.", "PausaPatch must run as administrator.", "PausaPatch muss als Administrator ausgeführt werden.", "PausaPatch doit être exécuté en tant qu'administrateur.", "PausaPatch deve essere eseguito come amministratore.", "PausaPatch 需要以管理员身份运行。", "PausaPatch は管理者として実行する必要があります。", "PausaPatch должен быть запущен от имени администратора.";
    error_generic: "La operación falló.", "The operation failed.", "Der Vorgang ist fehlgeschlagen.", "L'opération a échoué.", "Operazione non riuscita.", "操作失败。", "操作に失敗しました。", "Операция не выполнена.";
    log_title: "Registro", "Log", "Protokoll", "Journal", "Registro", "日志", "ログ", "Журнал";
    log_empty: "No hay eventos todavía.", "No events yet.", "Noch keine Ereignisse.", "Aucun événement pour le moment.", "Nessun evento.", "暂无事件。", "イベントはまだありません。", "Событий пока нет.";
    copy_all: "Copiar todo", "Copy all", "Alles kopieren", "Tout copier", "Copia tutto", "全部复制", "すべてコピー", "Копировать всё";
    about: "Acerca de", "About", "Info", "À propos", "Informazioni", "关于", "バージョン情報", "О программе";
    about_tagline: "Pausa las actualizaciones automáticas de Windows 11 de forma reversible.", "Pauses Windows 11 automatic updates in a reversible way.", "Pausiert Windows 11-Updates auf umkehrbare Weise.", "Suspend les mises à jour automatiques de Windows 11 de façon réversible.", "Sospende gli aggiornamenti automatici di Windows 11 in modo reversibile.", "以可逆方式暂停 Windows 11 自动更新。", "Windows 11 の自動更新を元に戻せる形で一時停止します。", "Приостанавливает автоматические обновления Windows 11 обратимым образом.";
    about_project: "Página del proyecto", "Project page", "Projektseite", "Page du projet", "Pagina del progetto", "项目主页", "プロジェクトページ", "Страница проекта";
    about_author_page: "Página del autor", "Author page", "Autorenseite", "Page de l'auteur", "Pagina dell'autore", "作者主页", "作者ページ", "Страница автора";
    about_author: "Autor", "Author", "Autor", "Auteur", "Autore", "作者", "作成者", "Автор";
    about_license: "Licencia", "License", "Lizenz", "Licence", "Licenza", "许可证", "ライセンス", "Лицензия";
    about_built_with: "Tecnología", "Built with", "Technik", "Technologie", "Tecnologia", "技术", "技術", "Технология";
    log_open_folder: "Abrir carpeta", "Open folder", "Ordner öffnen", "Ouvrir le dossier", "Apri cartella", "打开文件夹", "フォルダーを開く", "Открыть папку";
    log_clear: "Vaciar registro", "Clear log", "Protokoll leeren", "Vider le journal", "Svuota registro", "清空日志", "ログを消去", "Очистить журнал";
    log_clear_confirm: "¿Seguro que quieres vaciar el registro? Esta acción no se puede deshacer.", "Clear the log? This cannot be undone.", "Protokoll wirklich leeren? Das lässt sich nicht rückgängig machen.", "Vider le journal ? Cette action est irréversible.", "Svuotare il registro? L'operazione è irreversibile.", "确定要清空日志吗？此操作无法撤销。", "ログを消去しますか？この操作は取り消せません。", "Очистить журнал? Это действие необратимо.";
    cancel: "Cancelar", "Cancel", "Abbrechen", "Annuler", "Annulla", "取消", "キャンセル", "Отмена";
    log_lines: "{n} líneas", "{n} lines", "{n} Zeilen", "{n} lignes", "{n} righe", "{n} 行", "{n} 行", "{n} строк";
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lang {
    Es,
    #[default]
    En,
    De,
    Fr,
    It,
    ZhHans,
    Ja,
    Ru,
}

impl Lang {
    pub const ALL: [Lang; 8] = [
        Lang::Es,
        Lang::En,
        Lang::De,
        Lang::Fr,
        Lang::It,
        Lang::ZhHans,
        Lang::Ja,
        Lang::Ru,
    ];

    pub const fn code(self) -> &'static str {
        match self {
            Self::Es => "es",
            Self::En => "en",
            Self::De => "de",
            Self::Fr => "fr",
            Self::It => "it",
            Self::ZhHans => "zh-Hans",
            Self::Ja => "ja",
            Self::Ru => "ru",
        }
    }

    pub const fn native_name(self) -> &'static str {
        match self {
            Self::Es => "Español",
            Self::En => "English",
            Self::De => "Deutsch",
            Self::Fr => "Français",
            Self::It => "Italiano",
            Self::ZhHans => "简体中文",
            Self::Ja => "日本語",
            Self::Ru => "Русский",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        let code = code.to_ascii_lowercase();
        match code.as_str() {
            "es" | "es-es" => Some(Self::Es),
            "en" | "en-us" | "en-gb" => Some(Self::En),
            "de" | "de-de" => Some(Self::De),
            "fr" | "fr-fr" => Some(Self::Fr),
            "it" | "it-it" => Some(Self::It),
            "zh" | "zh-hans" | "zh-cn" | "zh-sg" => Some(Self::ZhHans),
            "ja" | "ja-jp" => Some(Self::Ja),
            "ru" | "ru-ru" => Some(Self::Ru),
            _ => None,
        }
    }

    pub fn detect() -> Self {
        let language_id = unsafe { GetUserDefaultUILanguage() };
        match language_id & 0x03ff {
            0x0a => Self::Es,
            0x09 => Self::En,
            0x07 => Self::De,
            0x0c => Self::Fr,
            0x10 => Self::It,
            0x04 => Self::ZhHans,
            0x11 => Self::Ja,
            0x19 => Self::Ru,
            _ => Self::En,
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|lang| *lang == self).unwrap_or(1)
    }

    pub fn strings(self) -> &'static Strings {
        match self {
            Self::Es => &ES,
            Self::En => &EN,
            Self::De => &DE,
            Self::Fr => &FR,
            Self::It => &IT,
            Self::ZhHans => &ZH,
            Self::Ja => &JA,
            Self::Ru => &RU,
        }
    }

    pub fn service_state_label(self, start: Option<u32>) -> &'static str {
        let s = self.strings();
        match start {
            Some(4) => s.disabled,
            Some(2) => s.automatic,
            Some(3) => s.manual,
            _ => s.unconfigured,
        }
    }

    pub fn paused_since(self, date: &str) -> String {
        self.strings().paused_since.replace("{date}", date)
    }

    pub fn reactivated_since(self, date: &str) -> String {
        self.strings().reactivated_since.replace("{date}", date)
    }

    pub fn mixed_since(self, date: &str) -> String {
        self.strings().mixed_since.replace("{date}", date)
    }

    pub fn checked_at(self, date: &str) -> String {
        self.strings().checked_at.replace("{date}", date)
    }

    pub fn check_interval(self, seconds: u64) -> String {
        self.strings()
            .check_interval
            .replace("{n}", &seconds.to_string())
    }

    pub fn tasks_disabled(self, count: usize) -> String {
        self.strings()
            .tasks_disabled
            .replace("{n}", &count.to_string())
    }

    pub fn log_lines(self, count: usize) -> String {
        self.strings().log_lines.replace("{n}", &count.to_string())
    }

    pub fn tasks_partial(self, enabled: usize, disabled: usize, paused: bool) -> String {
        let template = if paused {
            self.strings().tasks_partial
        } else {
            self.strings().tasks_partial_active
        };
        template
            .replace("{enabled}", &enabled.to_string())
            .replace("{disabled}", &disabled.to_string())
    }

    pub fn tasks_enabled(self, count: usize) -> String {
        self.strings()
            .tasks_enabled
            .replace("{n}", &count.to_string())
    }

    pub fn format_date(self, timestamp: i64) -> String {
        use chrono::{Datelike, Local, TimeZone, Timelike};

        let moment = Local
            .timestamp_opt(timestamp, 0)
            .single()
            .unwrap_or_else(Local::now);
        let month = self.months()[moment.month0() as usize];
        match self {
            Self::En => format!(
                "{} {}, {} · {:02}:{:02}",
                month,
                moment.day(),
                moment.year(),
                moment.hour(),
                moment.minute()
            ),
            Self::ZhHans | Self::Ja => format!(
                "{}年{}月{}日 · {:02}:{:02}",
                moment.year(),
                moment.month(),
                moment.day(),
                moment.hour(),
                moment.minute()
            ),
            _ => format!(
                "{} {} {} · {:02}:{:02}",
                moment.day(),
                month,
                moment.year(),
                moment.hour(),
                moment.minute()
            ),
        }
    }

    fn months(self) -> [&'static str; 12] {
        match self {
            Self::Es => [
                "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic",
            ],
            Self::En => [
                "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
            ],
            Self::De => [
                "Jan.", "Feb.", "März", "Apr.", "Mai", "Juni", "Juli", "Aug.", "Sept.", "Okt.",
                "Nov.", "Dez.",
            ],
            Self::Fr => [
                "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.",
                "nov.", "déc.",
            ],
            Self::It => [
                "gen.", "feb.", "mar.", "apr.", "mag.", "giu.", "lug.", "ago.", "set.", "ott.",
                "nov.", "dic.",
            ],
            Self::ZhHans => [
                "1月", "2月", "3月", "4月", "5月", "6月", "7月", "8月", "9月", "10月", "11月",
                "12月",
            ],
            Self::Ja => [
                "1月", "2月", "3月", "4月", "5月", "6月", "7月", "8月", "9月", "10月", "11月",
                "12月",
            ],
            Self::Ru => [
                "янв.",
                "февр.",
                "мар.",
                "апр.",
                "мая",
                "июн.",
                "июл.",
                "авг.",
                "сент.",
                "окт.",
                "нояб.",
                "дек.",
            ],
        }
    }
}

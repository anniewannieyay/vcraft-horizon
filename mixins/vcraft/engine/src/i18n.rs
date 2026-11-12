//! VCraft's translations: the English text is the key, and the languages
//! Horizon's launcher offers have their own. Missing text stays English.
//! The language follows the game's unless VCraft's settings pick one.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The languages, by code, in table order after English.
pub const LANGS: [(&str, &str); 8] = [
    ("en", "English"),
    ("fr", "Français"),
    ("de", "Deutsch"),
    ("es", "Español"),
    ("ru", "Русский"),
    ("zh", "中文"),
    ("uk", "Українська"),
    ("pl", "Polski"),
];

static CURRENT: AtomicUsize = AtomicUsize::new(0);

/// Tests that switch the language hold this, so they don't race.
#[cfg(test)]
pub static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Picks the language from a code: `fr`, `fr_fr`, `de_at`... Unknown means English.
pub fn set(code: &str) {
    let short = code.split(['_', '-']).next().unwrap_or("").to_lowercase();
    let i = LANGS.iter().position(|(c, _)| *c == short).unwrap_or(0);
    CURRENT.store(i, Ordering::Relaxed);
}

/// The language's own name for a code.
pub fn name(code: &str) -> &'static str {
    LANGS.iter().find(|(c, _)| *c == code).map_or("English", |(_, n)| n)
}

/// The language setting after (or before) `current`: "game", then each language.
pub fn cycle(current: &str, forward: bool) -> &'static str {
    let mut all: Vec<&'static str> = vec!["game"];
    all.extend(LANGS.iter().map(|(c, _)| *c));
    let i = all.iter().position(|c| *c == current).unwrap_or(0);
    let n = all.len();
    all[if forward { (i + 1) % n } else { (i + n - 1) % n }]
}

/// `text` in the current language.
pub fn t(text: &'static str) -> &'static str {
    let i = CURRENT.load(Ordering::Relaxed);
    if i == 0 {
        return text;
    }
    TABLE.get(text).map_or(text, |row| row[i - 1])
}

/// `text` translated, with `{0}`, `{1}`... filled in.
pub fn tf(text: &'static str, args: &[&str]) -> String {
    let mut out = t(text).to_owned();
    for (i, a) in args.iter().enumerate() {
        out = out.replace(&format!("{{{i}}}"), a);
    }
    out
}

static TABLE: LazyLock<HashMap<&'static str, [&'static str; 7]>> = LazyLock::new(|| ROWS.iter().map(|(k, v)| (*k, *v)).collect());

// English: [French, German, Spanish, Russian, Chinese, Ukrainian, Polish]
#[rustfmt::skip]
const ROWS: &[(&str, [&str; 7])] = &[
    // Statuses
    ("Online", ["En ligne", "Online", "En línea", "В сети", "在线", "У мережі", "Dostępny"]),
    ("Idle", ["Absent", "Abwesend", "Ausente", "Отошёл", "离开", "Відійшов", "Zaraz wracam"]),
    ("DND", ["NPD", "NS", "NM", "НБ", "勿扰", "НТ", "NP"]),
    ("Do Not Disturb", ["Ne pas déranger", "Nicht stören", "No molestar", "Не беспокоить", "请勿打扰", "Не турбувати", "Nie przeszkadzać"]),
    ("Offline", ["Hors ligne", "Offline", "Desconectado", "Не в сети", "离线", "Не в мережі", "Niedostępny"]),
    ("Offline (invisible)", ["Hors ligne (invisible)", "Offline (unsichtbar)", "Desconectado (invisible)", "Не в сети (невидимка)", "离线（隐身）", "Не в мережі (невидимка)", "Niedostępny (niewidoczny)"]),
    ("Status", ["Statut", "Status", "Estado", "Статус", "状态", "Статус", "Status"]),
    // Sidebar
    ("Friends", ["Amis", "Freunde", "Amigos", "Друзья", "好友", "Друзі", "Znajomi"]),
    ("VCraft Community", ["Communauté VCraft", "VCraft-Community", "Comunidad VCraft", "Сообщество VCraft", "VCraft 社区", "Спільнота VCraft", "Społeczność VCraft"]),
    ("DIRECT MESSAGES", ["MESSAGES PRIVÉS", "DIREKTNACHRICHTEN", "MENSAJES DIRECTOS", "ЛИЧНЫЕ СООБЩЕНИЯ", "私信", "ОСОБИСТІ ПОВІДОМЛЕННЯ", "WIADOMOŚCI PRYWATNE"]),
    ("GROUPS", ["GROUPES", "GRUPPEN", "GRUPOS", "ГРУППЫ", "群组", "ГРУПИ", "GRUPY"]),
    ("No friends yet", ["Pas encore d'amis", "Noch keine Freunde", "Aún no tienes amigos", "Пока нет друзей", "还没有好友", "Поки немає друзів", "Brak znajomych"]),
    ("No groups yet", ["Pas encore de groupes", "Noch keine Gruppen", "Aún no hay grupos", "Пока нет групп", "还没有群组", "Поки немає груп", "Brak grup"]),
    ("{0} of {1} online", ["{0} sur {1} en ligne", "{0} von {1} online", "{0} de {1} en línea", "{0} из {1} в сети", "{1} 人中 {0} 人在线", "{0} з {1} у мережі", "{0} z {1} dostępnych"]),
    ("Muted", ["Muet", "Stumm", "Silenciado", "Без звука", "已静音", "Без звуку", "Wyciszony"]),
    ("muted", ["muet", "stumm", "silenciado", "без звука", "已静音", "без звуку", "wyciszony"]),
    ("Deafened", ["Sourdine", "Taub", "Ensordecido", "Звук выключен", "已闭麦闭听", "Звук вимкнено", "Ogłuszony"]),
    ("deafened", ["sourdine", "taub", "ensordecido", "звук выключен", "已闭听", "звук вимкнено", "ogłuszony"]),
    ("muted for you", ["muet pour vous", "für dich stumm", "silenciado para ti", "заглушён для вас", "已为你静音", "заглушено для вас", "wyciszony dla ciebie"]),
    ("is calling...", ["appelle...", "ruft an...", "está llamando...", "звонит...", "正在呼叫...", "дзвонить...", "dzwoni..."]),
    ("wants to join your call", ["veut rejoindre votre appel", "möchte deinem Anruf beitreten", "quiere unirse a tu llamada", "хочет присоединиться к звонку", "想加入你的通话", "хоче приєднатися до дзвінка", "chce dołączyć do rozmowy"]),
    ("wants to join {0}", ["veut rejoindre {0}", "möchte {0} beitreten", "quiere unirse a {0}", "хочет вступить в {0}", "想加入 {0}", "хоче приєднатися до {0}", "chce dołączyć do {0}"]),
    ("Voice Connected", ["Vocal connecté", "Sprache verbunden", "Voz conectada", "Голос подключён", "语音已连接", "Голос підключено", "Głos połączony"]),
    ("Waiting for others...", ["En attente des autres...", "Warte auf andere...", "Esperando a los demás...", "Ждём остальных...", "等待其他人...", "Чекаємо на інших...", "Czekanie na innych..."]),
    ("1 other in call", ["1 autre dans l'appel", "1 weitere Person im Anruf", "1 persona más en la llamada", "Ещё 1 в звонке", "通话中还有 1 人", "Ще 1 у дзвінку", "1 inna osoba w rozmowie"]),
    ("{0} others in call", ["{0} autres dans l'appel", "{0} weitere im Anruf", "{0} personas más en la llamada", "Ещё {0} в звонке", "通话中还有 {0} 人", "Ще {0} у дзвінку", "{0} innych w rozmowie"]),
    ("Not connected", ["Non connecté", "Nicht verbunden", "No conectado", "Нет подключения", "未连接", "Немає з'єднання", "Brak połączenia"]),
    // Friends page
    ("All", ["Tous", "Alle", "Todos", "Все", "全部", "Усі", "Wszyscy"]),
    ("Pending", ["En attente", "Ausstehend", "Pendientes", "Ожидают", "待处理", "Очікують", "Oczekujące"]),
    ("Blocked", ["Bloqués", "Blockiert", "Bloqueados", "Заблокированы", "已屏蔽", "Заблоковані", "Zablokowani"]),
    ("ONLINE", ["EN LIGNE", "ONLINE", "EN LÍNEA", "В СЕТИ", "在线", "У МЕРЕЖІ", "DOSTĘPNI"]),
    ("ALL FRIENDS", ["TOUS LES AMIS", "ALLE FREUNDE", "TODOS LOS AMIGOS", "ВСЕ ДРУЗЬЯ", "全部好友", "УСІ ДРУЗІ", "WSZYSCY ZNAJOMI"]),
    ("PENDING", ["EN ATTENTE", "AUSSTEHEND", "PENDIENTES", "ОЖИДАЮТ", "待处理", "ОЧІКУЮТЬ", "OCZEKUJĄCE"]),
    ("BLOCKED", ["BLOQUÉS", "BLOCKIERT", "BLOQUEADOS", "ЗАБЛОКИРОВАНЫ", "已屏蔽", "ЗАБЛОКОВАНІ", "ZABLOKOWANI"]),
    ("ADD FRIEND", ["AJOUTER UN AMI", "FREUND HINZUFÜGEN", "AÑADIR AMIGO", "ДОБАВИТЬ ДРУГА", "添加好友", "ДОДАТИ ДРУГА", "DODAJ ZNAJOMEGO"]),
    ("Send a request with their Minecraft username.", ["Envoyez une demande avec son pseudo Minecraft.", "Sende eine Anfrage mit dem Minecraft-Namen.", "Envía una solicitud con su nombre de Minecraft.", "Отправьте заявку по нику в Minecraft.", "用对方的 Minecraft 用户名发送请求。", "Надішліть запит за ніком у Minecraft.", "Wyślij zaproszenie, podając nick z Minecrafta."]),
    ("Enter a Minecraft username", ["Pseudo Minecraft", "Minecraft-Name eingeben", "Nombre de Minecraft", "Ник в Minecraft", "输入 Minecraft 用户名", "Нік у Minecraft", "Nick z Minecrafta"]),
    ("Send Request", ["Envoyer", "Anfrage senden", "Enviar", "Отправить", "发送请求", "Надіслати", "Wyślij"]),
    ("Paste a group invite code (vc1...)", ["Collez un code d'invitation de groupe (vc1...)", "Gruppen-Einladungscode einfügen (vc1...)", "Pega un código de invitación (vc1...)", "Вставьте код приглашения в группу (vc1...)", "粘贴群组邀请码 (vc1...)", "Вставте код запрошення до групи (vc1...)", "Wklej kod zaproszenia do grupy (vc1...)"]),
    ("Join Group", ["Rejoindre", "Beitreten", "Unirse", "Вступить", "加入群组", "Приєднатися", "Dołącz"]),
    ("None of your friends are online right now.", ["Aucun de vos amis n'est en ligne.", "Gerade ist kein Freund online.", "Ninguno de tus amigos está en línea.", "Сейчас никого из друзей нет в сети.", "现在没有好友在线。", "Зараз нікого з друзів немає в мережі.", "Żaden znajomy nie jest teraz dostępny."]),
    ("Add a friend above to get started.", ["Ajoutez un ami ci-dessus pour commencer.", "Füge oben einen Freund hinzu.", "Añade un amigo arriba para empezar.", "Добавьте друга выше, чтобы начать.", "在上方添加好友开始吧。", "Додайте друга вище, щоб почати.", "Dodaj znajomego powyżej, aby zacząć."]),
    ("No pending friend requests.", ["Aucune demande en attente.", "Keine offenen Anfragen.", "No hay solicitudes pendientes.", "Нет заявок в друзья.", "没有待处理的好友请求。", "Немає запитів у друзі.", "Brak oczekujących zaproszeń."]),
    ("You haven't blocked anyone.", ["Vous n'avez bloqué personne.", "Du hast niemanden blockiert.", "No has bloqueado a nadie.", "Вы никого не заблокировали.", "你没有屏蔽任何人。", "Ви нікого не заблокували.", "Nikogo nie zablokowałeś."]),
    ("Support me", ["Me soutenir", "Unterstützen", "Apóyame", "Поддержать", "支持我", "Підтримати", "Wesprzyj mnie"]),
    ("Incoming friend request", ["Demande d'ami reçue", "Eingehende Anfrage", "Solicitud recibida", "Входящая заявка", "收到的好友请求", "Вхідний запит", "Otrzymane zaproszenie"]),
    ("Outgoing friend request", ["Demande d'ami envoyée", "Gesendete Anfrage", "Solicitud enviada", "Исходящая заявка", "已发送的好友请求", "Вихідний запит", "Wysłane zaproszenie"]),
    ("key changed, verify!", ["clé changée, vérifiez !", "Schlüssel geändert, prüfen!", "la clave cambió, ¡verifícala!", "ключ изменился, проверьте!", "密钥已更改，请验证！", "ключ змінився, перевірте!", "klucz się zmienił, sprawdź!"]),
    ("no encryption key yet", ["pas encore de clé de chiffrement", "noch kein Schlüssel", "aún sin clave de cifrado", "ещё нет ключа шифрования", "还没有加密密钥", "ще немає ключа шифрування", "brak klucza szyfrowania"]),
    ("Unblock", ["Débloquer", "Entsperren", "Desbloquear", "Разблокировать", "取消屏蔽", "Розблокувати", "Odblokuj"]),
    ("Ask to join their call", ["Demander à rejoindre son appel", "Fragen, ob du beitreten darfst", "Pedir unirse a su llamada", "Попроситься в звонок", "请求加入对方的通话", "Попроситися в дзвінок", "Poproś o dołączenie do rozmowy"]),
    ("Last seen {0}", ["Vu {0}", "Zuletzt gesehen {0}", "Visto {0}", "Был(а) {0}", "最后在线：{0}", "Був(ла) {0}", "Ostatnio {0}"]),
    ("just now", ["à l'instant", "gerade eben", "ahora mismo", "только что", "刚刚", "щойно", "przed chwilą"]),
    ("{0}m ago", ["il y a {0} min", "vor {0} Min.", "hace {0} min", "{0} мин назад", "{0} 分钟前", "{0} хв тому", "{0} min temu"]),
    ("{0}h ago", ["il y a {0} h", "vor {0} Std.", "hace {0} h", "{0} ч назад", "{0} 小时前", "{0} год тому", "{0} godz. temu"]),
    ("{0}d ago", ["il y a {0} j", "vor {0} Tagen", "hace {0} d", "{0} дн назад", "{0} 天前", "{0} дн тому", "{0} dni temu"]),
    ("In a call", ["En appel", "Im Anruf", "En una llamada", "В звонке", "通话中", "У дзвінку", "W rozmowie"]),
    ("In a call, in single player", ["En appel, en solo", "Im Anruf, Einzelspieler", "En una llamada, un jugador", "В звонке, одиночная игра", "通话中，单人游戏", "У дзвінку, одиночна гра", "W rozmowie, tryb jednoosobowy"]),
    ("In a call, on {0}", ["En appel, sur {0}", "Im Anruf, auf {0}", "En una llamada, en {0}", "В звонке, на {0}", "通话中，在 {0}", "У дзвінку, на {0}", "W rozmowie, na {0}"]),
    ("Playing single player", ["Joue en solo", "Spielt Einzelspieler", "Jugando en un jugador", "Играет в одиночку", "正在单人游戏", "Грає наодинці", "Gra w trybie jednoosobowym"]),
    ("Playing on {0}", ["Joue sur {0}", "Spielt auf {0}", "Jugando en {0}", "Играет на {0}", "正在 {0} 游玩", "Грає на {0}", "Gra na {0}"]),
    // Chat
    ("Your pinned messages ({0})", ["Vos messages épinglés ({0})", "Deine angehefteten Nachrichten ({0})", "Tus mensajes fijados ({0})", "Ваши закреплённые ({0})", "你置顶的消息（{0}）", "Ваші закріплені ({0})", "Twoje przypięte ({0})"]),
    ("YOUR PINS", ["VOS ÉPINGLES", "DEINE PINS", "TUS FIJADOS", "ЗАКРЕПЛЁННЫЕ", "我的置顶", "ЗАКРІПЛЕНІ", "PRZYPIĘTE"]),
    ("Only you see these.", ["Vous seul les voyez.", "Nur du siehst sie.", "Solo tú los ves.", "Видны только вам.", "只有你能看到。", "Бачите лише ви.", "Widzisz je tylko ty."]),
    ("Hover a message and click the pin to keep it here.", ["Survolez un message et cliquez sur l'épingle pour le garder ici.", "Fahre über eine Nachricht und klicke auf die Nadel.", "Pasa el ratón sobre un mensaje y pulsa la chincheta.", "Наведите на сообщение и нажмите на булавку.", "将鼠标悬停在消息上并点击图钉即可保存在这里。", "Наведіть на повідомлення й натисніть шпильку.", "Najedź na wiadomość i kliknij pinezkę."]),
    ("Unpin", ["Désépingler", "Lösen", "Desfijar", "Открепить", "取消置顶", "Відкріпити", "Odepnij"]),
    ("Pin for yourself", ["Épingler pour vous", "Für dich anheften", "Fijar para ti", "Закрепить для себя", "为自己置顶", "Закріпити для себе", "Przypnij dla siebie"]),
    ("Today at {0}", ["Aujourd'hui à {0}", "Heute um {0}", "Hoy a las {0}", "Сегодня в {0}", "今天 {0}", "Сьогодні о {0}", "Dziś o {0}"]),
    ("Yesterday at {0}", ["Hier à {0}", "Gestern um {0}", "Ayer a las {0}", "Вчера в {0}", "昨天 {0}", "Вчора о {0}", "Wczoraj o {0}"]),
    // Call page
    ("Voice Call", ["Appel vocal", "Sprachanruf", "Llamada de voz", "Голосовой звонок", "语音通话", "Голосовий дзвінок", "Rozmowa głosowa"]),
    ("you", ["vous", "du", "tú", "вы", "你", "ви", "ty"]),
    ("key not trusted", ["clé non fiable", "Schlüssel nicht vertraut", "clave no confiable", "ключ не доверен", "密钥未受信任", "ключ не довірений", "klucz niezaufany"]),
    ("securing...", ["sécurisation...", "wird gesichert...", "asegurando...", "защита...", "加密中...", "захист...", "zabezpieczanie..."]),
    ("Ringing...", ["Sonnerie...", "Klingelt...", "Sonando...", "Вызов...", "呼叫中...", "Виклик...", "Dzwonienie..."]),
    ("Ringing {0}...", ["Appel de {0}...", "Rufe {0} an...", "Llamando a {0}...", "Звоним {0}...", "正在呼叫 {0}...", "Дзвонимо {0}...", "Dzwonienie do {0}..."]),
    ("ASKING TO JOIN", ["DEMANDES POUR REJOINDRE", "MÖCHTEN BEITRETEN", "PIDEN UNIRSE", "ПРОСЯТСЯ В ЗВОНОК", "请求加入", "ПРОСЯТЬСЯ В ДЗВІНОК", "CHCĄ DOŁĄCZYĆ"]),
    ("Let in", ["Accepter", "Reinlassen", "Dejar entrar", "Впустить", "允许", "Впустити", "Wpuść"]),
    ("Ignore", ["Ignorer", "Ignorieren", "Ignorar", "Игнорировать", "忽略", "Ігнорувати", "Ignoruj"]),
    ("INVITE TO CALL", ["INVITER À L'APPEL", "ZUM ANRUF EINLADEN", "INVITAR A LA LLAMADA", "ПРИГЛАСИТЬ В ЗВОНОК", "邀请加入通话", "ЗАПРОСИТИ В ДЗВІНОК", "ZAPROŚ DO ROZMOWY"]),
    ("Nobody else is online.", ["Personne d'autre n'est en ligne.", "Sonst ist niemand online.", "Nadie más está en línea.", "Больше никого нет в сети.", "没有其他人在线。", "Більше нікого немає в мережі.", "Nikt inny nie jest dostępny."]),
    ("Reconnect voice", ["Reconnecter la voix", "Sprache neu verbinden", "Reconectar voz", "Переподключить голос", "重新连接语音", "Перепідключити голос", "Połącz głos ponownie"]),
    // Settings
    ("Settings", ["Paramètres", "Einstellungen", "Ajustes", "Настройки", "设置", "Налаштування", "Ustawienia"]),
    ("ACCOUNT", ["COMPTE", "KONTO", "CUENTA", "АККАУНТ", "账户", "АКАУНТ", "KONTO"]),
    ("Do Not Disturb silences pop-ups, sounds and ringing. Offline hides you.", ["Ne pas déranger coupe les notifications, les sons et la sonnerie. Hors ligne vous cache.", "Nicht stören schaltet Pop-ups, Töne und Klingeln stumm. Offline versteckt dich.", "No molestar silencia avisos, sonidos y llamadas. Desconectado te oculta.", "«Не беспокоить» отключает уведомления, звуки и вызовы. «Не в сети» скрывает вас.", "请勿打扰会关闭弹窗、声音和铃声。离线会隐藏你。", "«Не турбувати» вимикає сповіщення, звуки й дзвінки. «Не в мережі» приховує вас.", "Nie przeszkadzać wycisza powiadomienia, dźwięki i dzwonki. Niedostępny ukrywa cię."]),
    ("Language", ["Langue", "Sprache", "Idioma", "Язык", "语言", "Мова", "Język"]),
    ("Same as the game", ["Comme le jeu", "Wie im Spiel", "Igual que el juego", "Как в игре", "跟随游戏", "Як у грі", "Jak w grze"]),
    ("CONNECTION", ["CONNEXION", "VERBINDUNG", "CONEXIÓN", "ПОДКЛЮЧЕНИЕ", "连接", "З'ЄДНАННЯ", "POŁĄCZENIE"]),
    ("Relay address", ["Adresse du relais", "Relay-Adresse", "Dirección del relé", "Адрес релея", "中继地址", "Адреса релею", "Adres przekaźnika"]),
    ("Save", ["Enregistrer", "Speichern", "Guardar", "Сохранить", "保存", "Зберегти", "Zapisz"]),
    ("Connected as {0}", ["Connecté en tant que {0}", "Verbunden als {0}", "Conectado como {0}", "Вы вошли как {0}", "已作为 {0} 连接", "Підключено як {0}", "Połączono jako {0}"]),
    ("Connecting...", ["Connexion...", "Verbinde...", "Conectando...", "Подключение...", "连接中...", "Підключення...", "Łączenie..."]),
    ("Signing in...", ["Connexion au compte...", "Anmeldung...", "Iniciando sesión...", "Вход...", "登录中...", "Вхід...", "Logowanie..."]),
    ("Connect on startup", ["Se connecter au démarrage", "Beim Start verbinden", "Conectar al iniciar", "Подключаться при запуске", "启动时连接", "Підключатися під час запуску", "Łącz przy starcie"]),
    ("ABOUT", ["À PROPOS", "ÜBER", "ACERCA DE", "О МОДЕ", "关于", "ПРО МОД", "O MODZIE"]),
    ("VCraft {0} for Horizon", ["VCraft {0} pour Horizon", "VCraft {0} für Horizon", "VCraft {0} para Horizon", "VCraft {0} для Horizon", "VCraft {0}（Horizon 版）", "VCraft {0} для Horizon", "VCraft {0} dla Horizon"]),
    ("This is a Horizon native mod, so it updates through the Horizon launcher, not by itself.", ["C'est un mod natif Horizon : il se met à jour via le lanceur Horizon.", "Das ist ein nativer Horizon-Mod; er wird über den Horizon-Launcher aktualisiert.", "Es un mod nativo de Horizon: se actualiza desde el launcher de Horizon.", "Это нативный мод Horizon: он обновляется через лаунчер Horizon.", "这是 Horizon 原生模组，通过 Horizon 启动器更新。", "Це нативний мод Horizon: він оновлюється через лаунчер Horizon.", "To natywny mod Horizon, aktualizuje się przez launcher Horizon."]),
    ("VOICE", ["VOIX", "SPRACHE", "VOZ", "ГОЛОС", "语音", "ГОЛОС", "GŁOS"]),
    ("Input mode", ["Mode d'entrée", "Eingabemodus", "Modo de entrada", "Режим ввода", "输入模式", "Режим введення", "Tryb wejścia"]),
    ("Voice Activity", ["Activité vocale", "Sprachaktivität", "Actividad de voz", "Активация голосом", "语音激活", "Активація голосом", "Aktywacja głosem"]),
    ("Push to Talk", ["Appuyer pour parler", "Push-to-Talk", "Pulsar para hablar", "Рация", "按键说话", "Натисни й говори", "Naciśnij, aby mówić"]),
    ("Activation threshold", ["Seuil d'activation", "Aktivierungsschwelle", "Umbral de activación", "Порог активации", "激活阈值", "Поріг активації", "Próg aktywacji"]),
    ("Mic volume", ["Volume du micro", "Mikrofonlautstärke", "Volumen del micro", "Громкость микрофона", "麦克风音量", "Гучність мікрофона", "Głośność mikrofonu"]),
    ("Voice volume", ["Volume des voix", "Stimmenlautstärke", "Volumen de voces", "Громкость голосов", "语音音量", "Гучність голосів", "Głośność głosów"]),
    ("Noise suppression", ["Suppression du bruit", "Rauschunterdrückung", "Supresión de ruido", "Шумоподавление", "噪声抑制", "Шумозаглушення", "Tłumienie szumów"]),
    ("Echo cancellation", ["Annulation de l'écho", "Echounterdrückung", "Cancelación de eco", "Эхоподавление", "回声消除", "Ехозаглушення", "Usuwanie echa"]),
    ("Turn this on if you use speakers instead of headphones, so others don't hear themselves.", ["Activez-la si vous utilisez des haut-parleurs plutôt qu'un casque, pour que les autres ne s'entendent pas.", "Schalte das ein, wenn du Lautsprecher statt Kopfhörer nutzt, damit andere sich nicht selbst hören.", "Actívala si usas altavoces en vez de auriculares, para que los demás no se oigan a sí mismos.", "Включите, если у вас колонки, а не наушники, чтобы другие не слышали себя.", "如果你用扬声器而不是耳机，请开启，这样别人不会听到自己的回声。", "Увімкніть, якщо у вас колонки, а не навушники, щоб інші не чули себе.", "Włącz, jeśli używasz głośników zamiast słuchawek, aby inni nie słyszeli siebie."]),
    ("Voice effect", ["Effet de voix", "Stimmeffekt", "Efecto de voz", "Эффект голоса", "变声效果", "Ефект голосу", "Efekt głosu"]),
    ("None", ["Aucun", "Keiner", "Ninguno", "Нет", "无", "Немає", "Brak"]),
    ("Higher voice", ["Voix aiguë", "Höhere Stimme", "Voz aguda", "Высокий голос", "高音", "Високий голос", "Wyższy głos"]),
    ("Deeper voice", ["Voix grave", "Tiefere Stimme", "Voz grave", "Низкий голос", "低音", "Низький голос", "Niższy głos"]),
    ("Radio", ["Radio", "Funkgerät", "Radio", "Рация", "对讲机", "Рація", "Krótkofalówka"]),
    ("Mic test", ["Test du micro", "Mikrofontest", "Prueba de micro", "Проверка микрофона", "麦克风测试", "Перевірка мікрофона", "Test mikrofonu"]),
    ("Stop testing", ["Arrêter le test", "Test beenden", "Detener prueba", "Остановить", "停止测试", "Зупинити", "Zatrzymaj test"]),
    ("Let's check", ["Vérifier", "Testen", "Probar", "Проверить", "开始测试", "Перевірити", "Sprawdź"]),
    ("Mute while paused", ["Muet en pause", "Stumm im Pausenmenü", "Silenciar en pausa", "Без звука на паузе", "暂停时静音", "Без звуку на паузі", "Wycisz w pauzie"]),
    ("Mute in other windows", ["Muet dans les autres fenêtres", "Stumm in anderen Fenstern", "Silenciar en otras ventanas", "Без звука в других окнах", "切到其他窗口时静音", "Без звуку в інших вікнах", "Wycisz w innych oknach"]),
    ("Muted while paused", ["Muet (pause)", "Stumm (Pause)", "Silenciado (pausa)", "Без звука (пауза)", "已静音（暂停）", "Без звуку (пауза)", "Wyciszony (pauza)"]),
    ("Muted while in another window", ["Muet (autre fenêtre)", "Stumm (anderes Fenster)", "Silenciado (otra ventana)", "Без звука (другое окно)", "已静音（其他窗口）", "Без звуку (інше вікно)", "Wyciszony (inne okno)"]),
    ("CALLS", ["APPELS", "ANRUFE", "LLAMADAS", "ЗВОНКИ", "通话", "ДЗВІНКИ", "ROZMOWY"]),
    ("Rejoin calls after a dropped connection", ["Rejoindre l'appel après une coupure", "Nach Verbindungsabbruch wieder beitreten", "Volver a la llamada tras un corte", "Возвращаться в звонок после обрыва", "断线后重新加入通话", "Повертатися в дзвінок після обриву", "Wracaj do rozmowy po zerwaniu"]),
    ("Show friends my server and calls", ["Montrer mon serveur et mes appels", "Freunden Server und Anrufe zeigen", "Mostrar mi servidor y llamadas", "Показывать друзьям сервер и звонки", "向好友显示我的服务器和通话", "Показувати друзям сервер і дзвінки", "Pokazuj znajomym serwer i rozmowy"]),
    ("Friends on VCraft for Horizon see which server you're on and when you're in a call, and can ask to join. End-to-end encrypted; hidden while you're Invisible.", ["Vos amis sur VCraft pour Horizon voient votre serveur et vos appels, et peuvent demander à rejoindre. Chiffré de bout en bout ; masqué en mode invisible.", "Freunde mit VCraft für Horizon sehen deinen Server und deine Anrufe und können beitreten wollen. Ende-zu-Ende-verschlüsselt; unsichtbar ausgeblendet.", "Tus amigos en VCraft para Horizon ven tu servidor y tus llamadas, y pueden pedir unirse. Cifrado de extremo a extremo; oculto en modo invisible.", "Друзья с VCraft для Horizon видят ваш сервер и звонки и могут попроситься к вам. Сквозное шифрование; скрыто в режиме невидимки.", "使用 Horizon 版 VCraft 的好友能看到你在哪个服务器、是否在通话，并可请求加入。端到端加密；隐身时隐藏。", "Друзі з VCraft для Horizon бачать ваш сервер і дзвінки та можуть попроситися. Наскрізне шифрування; приховано в режимі невидимки.", "Znajomi z VCraft dla Horizon widzą twój serwer i rozmowy i mogą poprosić o dołączenie. Szyfrowanie end-to-end; ukryte, gdy jesteś niewidoczny."]),
    ("NOTIFICATIONS", ["NOTIFICATIONS", "BENACHRICHTIGUNGEN", "NOTIFICACIONES", "УВЕДОМЛЕНИЯ", "通知", "СПОВІЩЕННЯ", "POWIADOMIENIA"]),
    ("Pop-up notifications", ["Notifications", "Pop-up-Benachrichtigungen", "Notificaciones emergentes", "Всплывающие уведомления", "弹窗通知", "Спливаючі сповіщення", "Wyskakujące powiadomienia"]),
    ("Message sound", ["Son des messages", "Nachrichtenton", "Sonido de mensajes", "Звук сообщений", "消息提示音", "Звук повідомлень", "Dźwięk wiadomości"]),
    ("HUD", ["HUD", "HUD", "HUD", "HUD", "HUD", "HUD", "HUD"]),
    ("Compact call HUD (faces only)", ["HUD d'appel compact (visages)", "Kompaktes Anruf-HUD (nur Gesichter)", "HUD de llamada compacto (solo caras)", "Компактный HUD звонка (лица)", "紧凑通话 HUD（仅头像）", "Компактний HUD дзвінка (обличчя)", "Kompaktowy HUD rozmowy (twarze)"]),
    ("Unread badge while playing", ["Badge non lus en jeu", "Ungelesen-Hinweis beim Spielen", "Aviso de no leídos al jugar", "Счётчик непрочитанных в игре", "游戏中显示未读标记", "Лічильник непрочитаних у грі", "Licznik nieprzeczytanych w grze"]),
    ("1 unread message", ["1 message non lu", "1 ungelesene Nachricht", "1 mensaje sin leer", "1 непрочитанное сообщение", "1 条未读消息", "1 непрочитане повідомлення", "1 nieprzeczytana wiadomość"]),
    ("{0} unread messages", ["{0} messages non lus", "{0} ungelesene Nachrichten", "{0} mensajes sin leer", "Непрочитанных: {0}", "{0} 条未读消息", "Непрочитаних: {0}", "Nieprzeczytane: {0}"]),
    ("NOW LISTENING", ["EN ÉCOUTE", "LÄUFT GERADE", "ESCUCHANDO", "СЕЙЧАС ИГРАЕТ", "正在收听", "ЗАРАЗ ГРАЄ", "TERAZ SŁUCHAM"]),
    ("Show friends my song", ["Montrer ma musique", "Freunden meinen Song zeigen", "Mostrar mi canción", "Показывать друзьям мою музыку", "向好友显示我的歌曲", "Показувати друзям мою музику", "Pokazuj znajomym moją muzykę"]),
    ("Friends see what you're playing on your computer, end-to-end encrypted. Hidden while you're Invisible or when Now Playing's sharing is off.", ["Vos amis voient ce que vous écoutez, chiffré de bout en bout. Masqué en invisible ou si le partage de Now Playing est désactivé.", "Freunde sehen, was du hörst, Ende-zu-Ende-verschlüsselt. Ausgeblendet, wenn du unsichtbar bist oder Now Playing nicht teilt.", "Tus amigos ven lo que escuchas, cifrado de extremo a extremo. Oculto en modo invisible o si Now Playing no comparte.", "Друзья видят, что вы слушаете (сквозное шифрование). Скрыто в невидимке или если в Now Playing выключен обмен.", "好友能看到你正在播放的音乐（端到端加密）。隐身或关闭 Now Playing 分享时隐藏。", "Друзі бачать, що ви слухаєте (наскрізне шифрування). Приховано в невидимці або якщо в Now Playing вимкнено обмін.", "Znajomi widzą, czego słuchasz (szyfrowanie end-to-end). Ukryte, gdy jesteś niewidoczny lub Now Playing nie udostępnia."]),
    ("Turn on the Now Playing mod to show friends what you're listening to and share songs in chat.", ["Activez le mod Now Playing pour montrer votre musique et la partager dans le chat.", "Schalte den Mod Now Playing ein, um Freunden deine Musik zu zeigen und Songs zu teilen.", "Activa el mod Now Playing para mostrar lo que escuchas y compartir canciones.", "Включите мод Now Playing, чтобы показывать музыку и делиться песнями.", "开启 Now Playing 模组即可向好友展示和分享歌曲。", "Увімкніть мод Now Playing, щоб показувати музику й ділитися піснями.", "Włącz mod Now Playing, aby pokazywać muzykę i udostępniać utwory."]),
    ("Ringtone", ["Sonnerie", "Klingelton", "Tono de llamada", "Рингтон", "铃声", "Рингтон", "Dzwonek"]),
    ("Bell chime", ["Cloche", "Glocke", "Campana", "Колокольчик", "铃铛", "Дзвіночок", "Dzwonek"]),
    ("None (silent)", ["Aucune (silencieux)", "Keiner (still)", "Ninguno (silencio)", "Нет (тихо)", "无（静音）", "Немає (тихо)", "Brak (cisza)"]),
    ("Horizon can't stop a music disc once it starts, so calls ring with a repeating bell instead.", ["Horizon ne peut pas arrêter un disque une fois lancé : les appels sonnent avec une cloche.", "Horizon kann eine Schallplatte nicht stoppen, daher klingeln Anrufe mit einer Glocke.", "Horizon no puede parar un disco una vez empieza, así que las llamadas suenan con una campana.", "Horizon не умеет останавливать пластинку, поэтому звонки звучат колокольчиком.", "Horizon 无法停止已开始播放的唱片，所以来电会用铃铛声提示。", "Horizon не вміє зупиняти платівку, тому дзвінки звучать дзвіночком.", "Horizon nie potrafi zatrzymać płyty, więc rozmowy dzwonią dzwonkiem."]),
    ("PRIVACY", ["CONFIDENTIALITÉ", "DATENSCHUTZ", "PRIVACIDAD", "КОНФИДЕНЦИАЛЬНОСТЬ", "隐私", "ПРИВАТНІСТЬ", "PRYWATNOŚĆ"]),
    ("Blocked users", ["Utilisateurs bloqués", "Blockierte Nutzer", "Usuarios bloqueados", "Заблокированные", "已屏蔽的用户", "Заблоковані", "Zablokowani"]),
    ("{0} blocked - manage", ["{0} bloqués - gérer", "{0} blockiert - verwalten", "{0} bloqueados - gestionar", "Заблокировано: {0}", "已屏蔽 {0} 人 - 管理", "Заблоковано: {0}", "Zablokowani: {0}"]),
    ("Your data", ["Vos données", "Deine Daten", "Tus datos", "Ваши данные", "你的数据", "Ваші дані", "Twoje dane"]),
    ("Download my data", ["Télécharger", "Herunterladen", "Descargar", "Скачать", "下载我的数据", "Завантажити", "Pobierz"]),
    ("Delete account", ["Supprimer le compte", "Konto löschen", "Eliminar cuenta", "Удалить аккаунт", "删除账户", "Видалити акаунт", "Usuń konto"]),
    ("Messages and calls are end-to-end encrypted. Your key lives in config/vcraft - back it up to keep your identity on a new PC.", ["Messages et appels sont chiffrés de bout en bout. Votre clé est dans config/vcraft : sauvegardez-la pour garder votre identité sur un nouveau PC.", "Nachrichten und Anrufe sind Ende-zu-Ende-verschlüsselt. Dein Schlüssel liegt in config/vcraft; sichere ihn für einen neuen PC.", "Mensajes y llamadas tienen cifrado de extremo a extremo. Tu clave está en config/vcraft: guárdala para mantener tu identidad en otro PC.", "Сообщения и звонки защищены сквозным шифрованием. Ключ лежит в config/vcraft — сохраните его для нового ПК.", "消息和通话均端到端加密。你的密钥在 config/vcraft，备份它即可在新电脑上保留身份。", "Повідомлення й дзвінки захищені наскрізним шифруванням. Ключ лежить у config/vcraft — збережіть його для нового ПК.", "Wiadomości i rozmowy są szyfrowane end-to-end. Klucz jest w config/vcraft – zrób kopię, by zachować tożsamość na nowym PC."]),
    ("CONTROLS", ["COMMANDES", "STEUERUNG", "CONTROLES", "УПРАВЛЕНИЕ", "按键", "КЕРУВАННЯ", "STEROWANIE"]),
    ("Not bound", ["Non attribuée", "Nicht belegt", "Sin asignar", "Не назначено", "未绑定", "Не призначено", "Nieprzypisany"]),
    ("Open VCraft", ["Ouvrir VCraft", "VCraft öffnen", "Abrir VCraft", "Открыть VCraft", "打开 VCraft", "Відкрити VCraft", "Otwórz VCraft"]),
    ("Toggle Mute", ["Muet", "Stumm umschalten", "Silenciar", "Микрофон вкл/выкл", "切换静音", "Мікрофон увімк/вимк", "Wycisz/odcisz"]),
    ("Toggle Deafen", ["Sourdine", "Taub umschalten", "Ensordecer", "Звук вкл/выкл", "切换闭听", "Звук увімк/вимк", "Ogłusz/odgłusz"]),
    ("Accept Call", ["Accepter l'appel", "Anruf annehmen", "Aceptar llamada", "Принять звонок", "接听", "Прийняти дзвінок", "Odbierz"]),
    ("Hang Up / Decline", ["Raccrocher / Refuser", "Auflegen / Ablehnen", "Colgar / Rechazar", "Сбросить / Отклонить", "挂断 / 拒绝", "Завершити / Відхилити", "Rozłącz / Odrzuć"]),
    ("Toggle Compact HUD", ["HUD compact", "Kompaktes HUD umschalten", "HUD compacto", "Компактный HUD", "切换紧凑 HUD", "Компактний HUD", "Kompaktowy HUD"]),
    ("Change them in Options > Mods > VCraft.", ["Modifiez-les dans Options > Mods > VCraft.", "Ändere sie unter Optionen > Mods > VCraft.", "Cámbialos en Opciones > Mods > VCraft.", "Меняются в Настройки > Моды > VCraft.", "在 选项 > 模组 > VCraft 中修改。", "Змінюються в Налаштування > Моди > VCraft.", "Zmień je w Opcje > Mody > VCraft."]),
    ("Commands: /vc msg, call, join, invite, accept, decline, hangup, mute, deafen, status, add, nick", ["Commandes : /vc msg, call, join, invite, accept, decline, hangup, mute, deafen, status, add, nick", "Befehle: /vc msg, call, join, invite, accept, decline, hangup, mute, deafen, status, add, nick", "Comandos: /vc msg, call, join, invite, accept, decline, hangup, mute, deafen, status, add, nick", "Команды: /vc msg, call, join, invite, accept, decline, hangup, mute, deafen, status, add, nick", "命令：/vc msg, call, join, invite, accept, decline, hangup, mute, deafen, status, add, nick", "Команди: /vc msg, call, join, invite, accept, decline, hangup, mute, deafen, status, add, nick", "Komendy: /vc msg, call, join, invite, accept, decline, hangup, mute, deafen, status, add, nick"]),
    ("TROUBLESHOOTING", ["DÉPANNAGE", "FEHLERSUCHE", "SOLUCIÓN DE PROBLEMAS", "ДИАГНОСТИКА", "故障排查", "ДІАГНОСТИКА", "ROZWIĄZYWANIE PROBLEMÓW"]),
    ("Debug log", ["Journal de débogage", "Debug-Protokoll", "Registro de depuración", "Журнал отладки", "调试日志", "Журнал налагодження", "Dziennik debugowania"]),
    ("Log file", ["Fichier journal", "Protokolldatei", "Archivo de registro", "Файл журнала", "日志文件", "Файл журналу", "Plik dziennika"]),
    ("Open log folder", ["Ouvrir le dossier", "Ordner öffnen", "Abrir carpeta", "Открыть папку", "打开日志文件夹", "Відкрити теку", "Otwórz folder"]),
    ("config/vcraft/debug.log records connections, audio devices and calls, never what anyone says or types. Attach it to bug reports.", ["config/vcraft/debug.log enregistre les connexions, les périphériques audio et les appels, jamais ce qui est dit ou écrit. Joignez-le aux rapports de bug.", "config/vcraft/debug.log protokolliert Verbindungen, Audiogeräte und Anrufe, nie Gesagtes oder Geschriebenes. Häng es an Fehlerberichte an.", "config/vcraft/debug.log registra conexiones, dispositivos de audio y llamadas, nunca lo que se dice o escribe. Adjúntalo a los informes de errores.", "config/vcraft/debug.log записывает подключения, аудиоустройства и звонки, но не то, что говорят или пишут. Прикладывайте его к баг-репортам.", "config/vcraft/debug.log 记录连接、音频设备和通话，从不记录任何人说的或打的内容。报告问题时请附上它。", "config/vcraft/debug.log записує підключення, аудіопристрої та дзвінки, але не те, що кажуть чи пишуть. Додавайте його до звітів про помилки.", "config/vcraft/debug.log zapisuje połączenia, urządzenia audio i rozmowy, nigdy treść wypowiedzi ani wiadomości. Dołącz go do zgłoszeń błędów."]),
    // Disconnected
    ("Connecting to VCraft...", ["Connexion à VCraft...", "Verbinde mit VCraft...", "Conectando a VCraft...", "Подключение к VCraft...", "正在连接 VCraft...", "Підключення до VCraft...", "Łączenie z VCraft..."]),
    ("Signing in with your Minecraft account...", ["Connexion avec votre compte Minecraft...", "Anmeldung mit deinem Minecraft-Konto...", "Iniciando sesión con tu cuenta de Minecraft...", "Вход через аккаунт Minecraft...", "正在用 Minecraft 账户登录...", "Вхід через акаунт Minecraft...", "Logowanie kontem Minecraft..."]),
    ("You're not connected", ["Vous n'êtes pas connecté", "Du bist nicht verbunden", "No estás conectado", "Вы не подключены", "你尚未连接", "Ви не підключені", "Nie masz połączenia"]),
    ("Retry", ["Réessayer", "Erneut", "Reintentar", "Повторить", "重试", "Повторити", "Ponów"]),
    // Menus and modals
    ("Open chat", ["Ouvrir le chat", "Chat öffnen", "Abrir chat", "Открыть чат", "打开聊天", "Відкрити чат", "Otwórz czat"]),
    ("Call group", ["Appeler le groupe", "Gruppe anrufen", "Llamar al grupo", "Позвонить группе", "呼叫群组", "Подзвонити групі", "Zadzwoń do grupy"]),
    ("Invite to this server", ["Inviter sur ce serveur", "Auf diesen Server einladen", "Invitar a este servidor", "Пригласить на этот сервер", "邀请到此服务器", "Запросити на цей сервер", "Zaproś na ten serwer"]),
    ("Add people", ["Ajouter des personnes", "Leute hinzufügen", "Añadir personas", "Добавить людей", "添加成员", "Додати людей", "Dodaj osoby"]),
    ("Copy invite code", ["Copier le code d'invitation", "Einladungscode kopieren", "Copiar código de invitación", "Скопировать код приглашения", "复制邀请码", "Скопіювати код запрошення", "Kopiuj kod zaproszenia"]),
    ("Mute group", ["Rendre le groupe muet", "Gruppe stummschalten", "Silenciar grupo", "Заглушить группу", "静音群组", "Заглушити групу", "Wycisz grupę"]),
    ("Unmute group", ["Réactiver le groupe", "Gruppe laut schalten", "Activar grupo", "Включить группу", "取消静音群组", "Увімкнути групу", "Odcisz grupę"]),
    ("Rename group", ["Renommer le groupe", "Gruppe umbenennen", "Renombrar grupo", "Переименовать группу", "重命名群组", "Перейменувати групу", "Zmień nazwę grupy"]),
    ("Leave group", ["Quitter le groupe", "Gruppe verlassen", "Salir del grupo", "Покинуть группу", "退出群组", "Покинути групу", "Opuść grupę"]),
    ("Message", ["Message", "Nachricht", "Mensaje", "Написать", "发消息", "Написати", "Napisz"]),
    ("Call", ["Appeler", "Anrufen", "Llamar", "Позвонить", "呼叫", "Подзвонити", "Zadzwoń"]),
    ("Nickname and note", ["Surnom et note", "Spitzname und Notiz", "Apodo y nota", "Прозвище и заметка", "昵称和备注", "Прізвисько й нотатка", "Pseudonim i notatka"]),
    ("Nickname and note for {0}", ["Surnom et note pour {0}", "Spitzname und Notiz für {0}", "Apodo y nota para {0}", "Прозвище и заметка для {0}", "{0} 的昵称和备注", "Прізвисько й нотатка для {0}", "Pseudonim i notatka dla {0}"]),
    ("NICKNAME", ["SURNOM", "SPITZNAME", "APODO", "ПРОЗВИЩЕ", "昵称", "ПРІЗВИСЬКО", "PSEUDONIM"]),
    ("NOTE", ["NOTE", "NOTIZ", "NOTA", "ЗАМЕТКА", "备注", "НОТАТКА", "NOTATKA"]),
    ("Anything you want to remember", ["Ce que vous voulez retenir", "Was du dir merken willst", "Lo que quieras recordar", "Всё, что хотите запомнить", "你想记住的任何事", "Усе, що хочете запам'ятати", "Cokolwiek chcesz zapamiętać"]),
    ("Cancel", ["Annuler", "Abbrechen", "Cancelar", "Отмена", "取消", "Скасувати", "Anuluj"]),
    ("Verify encryption", ["Vérifier le chiffrement", "Verschlüsselung prüfen", "Verificar cifrado", "Проверить шифрование", "验证加密", "Перевірити шифрування", "Zweryfikuj szyfrowanie"]),
    ("Mute", ["Rendre muet", "Stummschalten", "Silenciar", "Заглушить", "静音", "Заглушити", "Wycisz"]),
    ("Unmute", ["Réactiver", "Laut schalten", "Quitar silencio", "Включить звук", "取消静音", "Увімкнути звук", "Odcisz"]),
    ("Mute for me", ["Muet pour moi", "Für mich stumm", "Silenciar para mí", "Заглушить для меня", "为我静音", "Заглушити для мене", "Wycisz dla mnie"]),
    ("Unmute for me", ["Réactiver pour moi", "Für mich laut", "Quitar silencio para mí", "Включить для меня", "为我取消静音", "Увімкнути для мене", "Odcisz dla mnie"]),
    ("Remove friend", ["Retirer l'ami", "Freund entfernen", "Eliminar amigo", "Удалить из друзей", "删除好友", "Видалити з друзів", "Usuń znajomego"]),
    ("Block", ["Bloquer", "Blockieren", "Bloquear", "Заблокировать", "屏蔽", "Заблокувати", "Zablokuj"]),
    // Notices, toasts, chat lines
    ("Incoming call", ["Appel entrant", "Eingehender Anruf", "Llamada entrante", "Входящий звонок", "来电", "Вхідний дзвінок", "Połączenie przychodzące"]),
    ("{0} is calling you", ["{0} vous appelle", "{0} ruft dich an", "{0} te está llamando", "{0} звонит вам", "{0} 正在呼叫你", "{0} дзвонить вам", "{0} dzwoni do ciebie"]),
    ("{0} is calling you. Type /vc accept or open VCraft.", ["{0} vous appelle. Tapez /vc accept ou ouvrez VCraft.", "{0} ruft dich an. Tippe /vc accept oder öffne VCraft.", "{0} te está llamando. Escribe /vc accept o abre VCraft.", "{0} звонит вам. Введите /vc accept или откройте VCraft.", "{0} 正在呼叫你。输入 /vc accept 或打开 VCraft。", "{0} дзвонить вам. Введіть /vc accept або відкрийте VCraft.", "{0} dzwoni do ciebie. Wpisz /vc accept lub otwórz VCraft."]),
    ("{0} in {1}", ["{0} dans {1}", "{0} in {1}", "{0} en {1}", "{0} в {1}", "{0}（{1}）", "{0} у {1}", "{0} w {1}"]),
    ("Request", ["Demande", "Anfrage", "Solicitud", "Запрос", "请求", "Запит", "Prośba"]),
    ("{0} wants to join your call", ["{0} veut rejoindre votre appel", "{0} möchte deinem Anruf beitreten", "{0} quiere unirse a tu llamada", "{0} хочет присоединиться к звонку", "{0} 想加入你的通话", "{0} хоче приєднатися до дзвінка", "{0} chce dołączyć do rozmowy"]),
    ("{0} wants to join {1}", ["{0} veut rejoindre {1}", "{0} möchte {1} beitreten", "{0} quiere unirse a {1}", "{0} хочет вступить в {1}", "{0} 想加入 {1}", "{0} хоче приєднатися до {1}", "{0} chce dołączyć do {1}"]),
    ("{0} wants to join a group", ["{0} veut rejoindre un groupe", "{0} möchte einer Gruppe beitreten", "{0} quiere unirse a un grupo", "{0} хочет вступить в группу", "{0} 想加入一个群组", "{0} хоче приєднатися до групи", "{0} chce dołączyć do grupy"]),
    ("Open VCraft to let them in.", ["Ouvrez VCraft pour l'accepter.", "Öffne VCraft, um zuzustimmen.", "Abre VCraft para dejarle entrar.", "Откройте VCraft, чтобы впустить.", "打开 VCraft 允许对方加入。", "Відкрийте VCraft, щоб впустити.", "Otwórz VCraft, aby wpuścić."]),
    ("{0} is rejoining the call", ["{0} rejoint l'appel", "{0} kehrt in den Anruf zurück", "{0} vuelve a la llamada", "{0} возвращается в звонок", "{0} 正在重新加入通话", "{0} повертається в дзвінок", "{0} wraca do rozmowy"]),
    ("{0} isn't in a call.", ["{0} n'est pas en appel.", "{0} ist in keinem Anruf.", "{0} no está en una llamada.", "{0} не в звонке.", "{0} 不在通话中。", "{0} не в дзвінку.", "{0} nie jest w rozmowie."]),
    ("Asked {0} to let you into their call.", ["Demande envoyée à {0} pour rejoindre son appel.", "{0} wurde gefragt, ob du beitreten darfst.", "Has pedido a {0} unirte a su llamada.", "Запрос отправлен {0}.", "已请求 {0} 让你加入通话。", "Запит надіслано {0}.", "Poproszono {0} o dołączenie."]),
    ("Couldn't get back into the call.", ["Impossible de rejoindre l'appel.", "Konnte dem Anruf nicht wieder beitreten.", "No se pudo volver a la llamada.", "Не удалось вернуться в звонок.", "无法重新加入通话。", "Не вдалося повернутися в дзвінок.", "Nie udało się wrócić do rozmowy."]),
    ("Reconnected. Getting you back into the call...", ["Reconnecté. Retour dans l'appel...", "Wieder verbunden. Du kommst zurück in den Anruf...", "Reconectado. Volviendo a la llamada...", "Подключение восстановлено. Возвращаем в звонок...", "已重新连接，正在让你回到通话...", "З'єднання відновлено. Повертаємо в дзвінок...", "Połączono ponownie. Wracasz do rozmowy..."]),
    ("Reconnected. Ask someone in the call to invite you again.", ["Reconnecté. Demandez à quelqu'un de vous réinviter.", "Wieder verbunden. Bitte jemanden im Anruf, dich neu einzuladen.", "Reconectado. Pide a alguien que te invite de nuevo.", "Подключение восстановлено. Попросите кого-нибудь пригласить вас снова.", "已重新连接。请让通话中的人再次邀请你。", "З'єднання відновлено. Попросіть когось запросити вас знову.", "Połączono ponownie. Poproś kogoś o ponowne zaproszenie."]),
    ("That isn't a VCraft group code.", ["Ce n'est pas un code de groupe VCraft.", "Das ist kein VCraft-Gruppencode.", "Eso no es un código de grupo de VCraft.", "Это не код группы VCraft.", "这不是 VCraft 群组邀请码。", "Це не код групи VCraft.", "To nie jest kod grupy VCraft."]),
    ("That's your own code.", ["C'est votre propre code.", "Das ist dein eigener Code.", "Ese es tu propio código.", "Это ваш собственный код.", "这是你自己的邀请码。", "Це ваш власний код.", "To twój własny kod."]),
    ("the group", ["le groupe", "die Gruppe", "el grupo", "группу", "该群组", "групу", "grupy"]),
    ("Asked {0} to add you to {1}.", ["Demande envoyée à {0} pour rejoindre {1}.", "{0} wurde gebeten, dich zu {1} hinzuzufügen.", "Has pedido a {0} que te añada a {1}.", "Попросили {0} добавить вас в {1}.", "已请求 {0} 把你加入 {1}。", "Попросили {0} додати вас до {1}.", "Poproszono {0} o dodanie cię do {1}."]),
    ("{0} is offline. Try again when they're online.", ["{0} est hors ligne. Réessayez plus tard.", "{0} ist offline. Versuch es später.", "{0} está desconectado. Inténtalo más tarde.", "{0} не в сети. Попробуйте позже.", "{0} 离线。请等对方上线后再试。", "{0} не в мережі. Спробуйте пізніше.", "{0} jest niedostępny. Spróbuj później."]),
    ("Sent {0} a friend request. Paste the code again once they accept.", ["Demande d'ami envoyée à {0}. Recollez le code une fois acceptée.", "Freundschaftsanfrage an {0} gesendet. Füge den Code nach der Annahme erneut ein.", "Solicitud enviada a {0}. Pega el código otra vez cuando acepte.", "Заявка отправлена {0}. Вставьте код снова, когда он(а) примет.", "已向 {0} 发送好友请求。对方接受后请再次粘贴邀请码。", "Запит надіслано {0}. Вставте код знову, коли він(вона) прийме.", "Wysłano zaproszenie do {0}. Wklej kod ponownie po akceptacji."]),
    ("Add the person who sent the code as a friend first.", ["Ajoutez d'abord la personne qui a envoyé le code.", "Füge zuerst die Person hinzu, die den Code geschickt hat.", "Primero añade como amigo a quien envió el código.", "Сначала добавьте в друзья того, кто прислал код.", "请先添加发送邀请码的人为好友。", "Спершу додайте в друзі того, хто надіслав код.", "Najpierw dodaj do znajomych osobę, która wysłała kod."]),
    ("Invite code copied. Friends paste it on the Friends page.", ["Code copié. Vos amis le collent sur la page Amis.", "Code kopiert. Freunde fügen ihn auf der Freunde-Seite ein.", "Código copiado. Tus amigos lo pegan en la página Amigos.", "Код скопирован. Друзья вставляют его на странице «Друзья».", "邀请码已复制。好友在“好友”页面粘贴即可。", "Код скопійовано. Друзі вставляють його на сторінці «Друзі».", "Kod skopiowany. Znajomi wklejają go na stronie Znajomi."]),
    ("Couldn't copy the code.", ["Impossible de copier le code.", "Code konnte nicht kopiert werden.", "No se pudo copiar el código.", "Не удалось скопировать код.", "无法复制邀请码。", "Не вдалося скопіювати код.", "Nie udało się skopiować kodu."]),
    ("Connect to VCraft first.", ["Connectez-vous d'abord à VCraft.", "Verbinde dich zuerst mit VCraft.", "Conéctate primero a VCraft.", "Сначала подключитесь к VCraft.", "请先连接 VCraft。", "Спершу підключіться до VCraft.", "Najpierw połącz się z VCraft."]),
    // Commands
    ("Usage: /vc msg|call|join|invite <friend>, accept, decline, hangup, mute, deafen, status <status>, add <player>, nick <friend> [nickname]", ["Usage : /vc msg|call|join|invite <ami>, accept, decline, hangup, mute, deafen, status <statut>, add <joueur>, nick <ami> [surnom]", "Verwendung: /vc msg|call|join|invite <Freund>, accept, decline, hangup, mute, deafen, status <Status>, add <Spieler>, nick <Freund> [Spitzname]", "Uso: /vc msg|call|join|invite <amigo>, accept, decline, hangup, mute, deafen, status <estado>, add <jugador>, nick <amigo> [apodo]", "Использование: /vc msg|call|join|invite <друг>, accept, decline, hangup, mute, deafen, status <статус>, add <игрок>, nick <друг> [прозвище]", "用法：/vc msg|call|join|invite <好友>, accept, decline, hangup, mute, deafen, status <状态>, add <玩家>, nick <好友> [昵称]", "Використання: /vc msg|call|join|invite <друг>, accept, decline, hangup, mute, deafen, status <статус>, add <гравець>, nick <друг> [прізвисько]", "Użycie: /vc msg|call|join|invite <znajomy>, accept, decline, hangup, mute, deafen, status <status>, add <gracz>, nick <znajomy> [pseudonim]"]),
    ("Usage: /vc nick <friend> [nickname] (no nickname clears it)", ["Usage : /vc nick <ami> [surnom] (sans surnom, il est effacé)", "Verwendung: /vc nick <Freund> [Spitzname] (ohne Spitzname wird er gelöscht)", "Uso: /vc nick <amigo> [apodo] (sin apodo se borra)", "Использование: /vc nick <друг> [прозвище] (без прозвища — сброс)", "用法：/vc nick <好友> [昵称]（不填昵称即清除）", "Використання: /vc nick <друг> [прізвисько] (без прізвиська — скидання)", "Użycie: /vc nick <znajomy> [pseudonim] (bez pseudonimu usuwa go)"]),
    ("{0} isn't on your VCraft friends list.", ["{0} n'est pas dans vos amis VCraft.", "{0} ist nicht in deiner VCraft-Freundesliste.", "{0} no está en tu lista de amigos de VCraft.", "{0} нет в ваших друзьях VCraft.", "{0} 不在你的 VCraft 好友列表中。", "{0} немає у ваших друзях VCraft.", "{0} nie ma na twojej liście znajomych VCraft."]),
    ("Cleared your nickname for {0}.", ["Surnom de {0} effacé.", "Spitzname für {0} entfernt.", "Apodo de {0} borrado.", "Прозвище для {0} сброшено.", "已清除 {0} 的昵称。", "Прізвисько для {0} скинуто.", "Usunięto pseudonim dla {0}."]),
    ("{0} is now {1} for you.", ["{0} s'appelle maintenant {1} pour vous.", "{0} heißt für dich jetzt {1}.", "{0} ahora es {1} para ti.", "Теперь {0} для вас — {1}.", "{0} 现在对你显示为 {1}。", "Тепер {0} для вас — {1}.", "{0} to teraz dla ciebie {1}."]),
    ("Invited {0} to this server.", ["{0} invité sur ce serveur.", "{0} auf diesen Server eingeladen.", "Has invitado a {0} a este servidor.", "{0} приглашён(а) на этот сервер.", "已邀请 {0} 来此服务器。", "{0} запрошено на цей сервер.", "Zaproszono {0} na ten serwer."]),
    ("Join a multiplayer server first, then invite people to it.", ["Rejoignez d'abord un serveur multijoueur, puis invitez-y des gens.", "Tritt zuerst einem Mehrspieler-Server bei und lade dann Leute ein.", "Únete primero a un servidor multijugador y luego invita a gente.", "Сначала зайдите на сервер, потом приглашайте.", "请先加入多人服务器，再邀请别人。", "Спершу зайдіть на сервер, потім запрошуйте.", "Najpierw dołącz do serwera, potem zapraszaj."]),
    ("Nobody is calling you.", ["Personne ne vous appelle.", "Niemand ruft dich an.", "Nadie te está llamando.", "Вам никто не звонит.", "没有人在呼叫你。", "Вам ніхто не дзвонить.", "Nikt do ciebie nie dzwoni."]),
    ("Microphone muted", ["Micro coupé", "Mikrofon stumm", "Micrófono silenciado", "Микрофон выключен", "麦克风已静音", "Мікрофон вимкнено", "Mikrofon wyciszony"]),
    ("Microphone unmuted", ["Micro activé", "Mikrofon an", "Micrófono activado", "Микрофон включён", "麦克风已开启", "Мікрофон увімкнено", "Mikrofon włączony"]),
    ("Undeafened", ["Son réactivé", "Ton wieder an", "Sonido activado", "Звук включён", "已恢复收听", "Звук увімкнено", "Dźwięk włączony"]),
    ("Use online, idle, dnd or offline.", ["Utilisez online, idle, dnd ou offline.", "Nutze online, idle, dnd oder offline.", "Usa online, idle, dnd u offline.", "Используйте online, idle, dnd или offline.", "请使用 online、idle、dnd 或 offline。", "Використовуйте online, idle, dnd або offline.", "Użyj online, idle, dnd lub offline."]),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_and_falls_back() {
        let _l = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set("fr_fr");
        assert_eq!(t("Friends"), "Amis");
        assert_eq!(tf("{0} is calling you", &["Alex"]), "Alex vous appelle");
        assert_eq!(t("Not in the table"), "Not in the table");
        set("pt_br");
        assert_eq!(t("Friends"), "Friends");
        set("en_us");
    }

    /// Every row keeps its placeholders in every language.
    #[test]
    fn placeholders_survive() {
        for (en, row) in ROWS {
            for n in 0..3 {
                let p = format!("{{{n}}}");
                if en.contains(&p) {
                    for tr in row {
                        assert!(tr.contains(&p), "{en:?} -> {tr:?} lost {p}");
                    }
                }
            }
        }
    }

    #[test]
    fn no_duplicate_rows() {
        let mut seen = std::collections::HashSet::new();
        for (en, _) in ROWS {
            assert!(seen.insert(*en), "{en:?} twice");
        }
    }

    #[test]
    fn cycles_through_languages() {
        assert_eq!(cycle("game", true), "en");
        assert_eq!(cycle("game", false), "pl");
        assert_eq!(name("de"), "Deutsch");
    }
}

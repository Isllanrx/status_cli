use super::Labels;

pub const ENGLISH: Labels = Labels {
    code: "en",
    session: "session",
    week: "week",
    compact: "compact",
    context: "context",
    quota: "quota",
    time: "time",
    resets_in: "resets in",
    limit_near: "Usage limit close: before starting long work, tell the user these percentages, keep the task notes \
                 current (done and next steps) and ask before starting subagents or workflows.",
    limit_reached: "Usage limit nearly exhausted: do not start new tasks. Record what is done and the next steps in \
                    the task notes, tell the user these percentages and when the limit resets, and stop.",
};

pub const PORTUGUESE: Labels = Labels {
    code: "pt-BR",
    session: "sessão",
    week: "semana",
    compact: "compactar",
    context: "contexto",
    quota: "cota",
    time: "tempo",
    resets_in: "reinicia em",
    limit_near: "Limite de uso próximo: antes de trabalho longo, avise o usuário com estes percentuais, mantenha as \
                 notas da tarefa atualizadas (feito e próximos passos) e confirme antes de abrir subagentes ou \
                 workflows.",
    limit_reached: "Limite de uso quase esgotado: não comece tarefas novas. Registre o que foi feito e os próximos \
                    passos nas notas da tarefa, avise o usuário com estes percentuais e quando o limite reinicia, e \
                    pare.",
};

pub const SPANISH: Labels = Labels {
    code: "es",
    session: "sesión",
    week: "semana",
    compact: "compactar",
    context: "contexto",
    quota: "cuota",
    time: "tiempo",
    resets_in: "se reinicia en",
    limit_near: "Límite de uso cercano: antes de un trabajo largo, informa al usuario de estos porcentajes, mantén al \
                 día las notas de la tarea (hecho y próximos pasos) y pide confirmación antes de iniciar subagentes o \
                 workflows.",
    limit_reached: "Límite de uso casi agotado: no empieces tareas nuevas. Registra lo hecho y los próximos pasos en \
                    las notas de la tarea, informa al usuario de estos porcentajes y de cuándo se reinicia el límite, \
                    y detente.",
};

pub const FRENCH: Labels = Labels {
    code: "fr",
    session: "session",
    week: "semaine",
    compact: "compacter",
    context: "contexte",
    quota: "quota",
    time: "temps",
    resets_in: "réinitialisé dans",
    limit_near: "Limite d'utilisation proche : avant un travail long, indique ces pourcentages à l'utilisateur, tiens \
                 à jour les notes de la tâche (fait et prochaines étapes) et demande confirmation avant de lancer des \
                 sous-agents ou des workflows.",
    limit_reached: "Limite d'utilisation presque atteinte : ne commence pas de nouvelle tâche. Note ce qui est fait \
                    et les prochaines étapes dans les notes de la tâche, indique ces pourcentages à l'utilisateur et \
                    quand la limite se réinitialise, puis arrête-toi.",
};

pub const GERMAN: Labels = Labels {
    code: "de",
    session: "Sitzung",
    week: "Woche",
    compact: "verdichten",
    context: "Kontext",
    quota: "Kontingent",
    time: "Zeit",
    resets_in: "zurückgesetzt in",
    limit_near: "Nutzungslimit nahe: Vor längerer Arbeit dem Nutzer diese Prozentwerte nennen, die Aufgabennotizen \
                 aktuell halten (erledigt und nächste Schritte) und vor dem Start von Subagenten oder Workflows \
                 nachfragen.",
    limit_reached: "Nutzungslimit fast erschöpft: Keine neuen Aufgaben beginnen. Erledigtes und nächste Schritte in \
                    den Aufgabennotizen festhalten, dem Nutzer diese Prozentwerte und den Zeitpunkt des \
                    Zurücksetzens nennen und aufhören.",
};

pub const ITALIAN: Labels = Labels {
    code: "it",
    session: "sessione",
    week: "settimana",
    compact: "compattare",
    context: "contesto",
    quota: "quota",
    time: "tempo",
    resets_in: "si azzera tra",
    limit_near: "Limite di utilizzo vicino: prima di un lavoro lungo, comunica all'utente queste percentuali, tieni \
                 aggiornate le note dell'attività (fatto e prossimi passi) e chiedi conferma prima di avviare \
                 subagenti o workflow.",
    limit_reached: "Limite di utilizzo quasi esaurito: non iniziare nuove attività. Registra ciò che è stato fatto e \
                    i prossimi passi nelle note dell'attività, comunica all'utente queste percentuali e quando il \
                    limite si azzera, poi fermati.",
};

pub const DUTCH: Labels = Labels {
    code: "nl",
    session: "sessie",
    week: "week",
    compact: "compacteren",
    context: "context",
    quota: "quotum",
    time: "tijd",
    resets_in: "reset over",
    limit_near: "Gebruikslimiet bijna bereikt: meld de gebruiker vóór langer werk deze percentages, houd de \
                 taaknotities bij (gedaan en volgende stappen) en vraag bevestiging voordat je subagents of \
                 workflows start.",
    limit_reached: "Gebruikslimiet bijna op: begin geen nieuwe taken. Leg vast wat gedaan is en de volgende stappen \
                    in de taaknotities, meld de gebruiker deze percentages en wanneer de limiet reset, en stop.",
};

pub const POLISH: Labels = Labels {
    code: "pl",
    session: "sesja",
    week: "tydzień",
    compact: "kompaktowanie",
    context: "kontekst",
    quota: "limit",
    time: "czas",
    resets_in: "reset za",
    limit_near: "Limit użycia blisko: przed dłuższą pracą podaj użytkownikowi te wartości procentowe, aktualizuj \
                 notatki zadania (zrobione i kolejne kroki) i pytaj o zgodę przed uruchomieniem subagentów lub \
                 workflowów.",
    limit_reached: "Limit użycia prawie wyczerpany: nie zaczynaj nowych zadań. Zapisz, co zrobiono, i kolejne kroki \
                    w notatkach zadania, podaj użytkownikowi te wartości procentowe i kiedy limit się odnowi, i \
                    zakończ.",
};

pub const INDONESIAN: Labels = Labels {
    code: "id",
    session: "sesi",
    week: "minggu",
    compact: "ringkas",
    context: "konteks",
    quota: "kuota",
    time: "waktu",
    resets_in: "reset dalam",
    limit_near: "Batas pemakaian hampir tercapai: sebelum pekerjaan panjang, beri tahu pengguna persentase ini, \
                 perbarui catatan tugas (selesai dan langkah berikutnya), dan minta konfirmasi sebelum menjalankan \
                 subagen atau workflow.",
    limit_reached: "Batas pemakaian hampir habis: jangan mulai tugas baru. Catat yang sudah selesai dan langkah \
                    berikutnya di catatan tugas, beri tahu pengguna persentase ini dan kapan batas direset, lalu \
                    berhenti.",
};

pub const TURKISH: Labels = Labels {
    code: "tr",
    session: "oturum",
    week: "hafta",
    compact: "sıkıştır",
    context: "bağlam",
    quota: "kota",
    time: "süre",
    resets_in: "sıfırlanmaya",
    limit_near: "Kullanım sınırı yakın: uzun bir işe başlamadan önce kullanıcıya bu yüzdeleri bildir, görev \
                 notlarını güncel tut (yapılanlar ve sonraki adımlar) ve alt ajan veya iş akışı başlatmadan önce \
                 onay iste.",
    limit_reached: "Kullanım sınırı neredeyse doldu: yeni görev başlatma. Yapılanları ve sonraki adımları görev \
                    notlarına yaz, kullanıcıya bu yüzdeleri ve sınırın ne zaman sıfırlanacağını bildir ve dur.",
};

pub const VIETNAMESE: Labels = Labels {
    code: "vi",
    session: "phiên",
    week: "tuần",
    compact: "nén",
    context: "ngữ cảnh",
    quota: "hạn mức",
    time: "thời gian",
    resets_in: "đặt lại sau",
    limit_near: "Sắp chạm giới hạn sử dụng: trước khi làm việc dài, hãy báo cho người dùng các phần trăm này, cập \
                 nhật ghi chú công việc (đã xong và bước tiếp theo) và hỏi xác nhận trước khi chạy subagent hoặc \
                 workflow.",
    limit_reached: "Giới hạn sử dụng gần cạn: không bắt đầu công việc mới. Ghi lại phần đã xong và các bước tiếp \
                    theo vào ghi chú công việc, báo cho người dùng các phần trăm này và thời điểm giới hạn được đặt \
                    lại, rồi dừng.",
};

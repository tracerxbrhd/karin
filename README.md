<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

<p align="center">
  <img src="resources/branding/karin-banner-montserrat.png" width="1086" alt="Karin, независимый музыкальный клиент для Linux">
</p>

<p align="center">
  <a href="https://github.com/tracerxbrhd/karin/releases/latest"><img src="https://img.shields.io/github/v/release/tracerxbrhd/karin?label=release&amp;style=flat" alt="Release"></a>
  <a href="https://github.com/tracerxbrhd/karin/actions/workflows/ci.yml?query=branch%3Amaster"><img src="https://img.shields.io/github/actions/workflow/status/tracerxbrhd/karin/ci.yml?branch=master&amp;label=CI&amp;style=flat" alt="CI"></a>
  <a href="https://github.com/tracerxbrhd/karin/stargazers"><img src="https://img.shields.io/github/stars/tracerxbrhd/karin?label=stars&amp;style=flat" alt="Stars"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/dynamic/xml?url=https%3A%2F%2Fraw.githubusercontent.com%2Ftracerxbrhd%2Fkarin%2Fmaster%2Fpackaging%2Fio.github.tracerxbrhd.Karin.metainfo.xml&amp;query=%2Fcomponent%2Fproject_license&amp;label=license&amp;color=blue&amp;style=flat" alt="Лицензия"></a>
  <a href="docs/BUILDING.md"><img src="https://img.shields.io/badge/platform-Linux-222222?logo=linux&amp;logoColor=white&amp;style=flat" alt="Linux"></a>
  <a href="#установка"><img src="https://img.shields.io/badge/package-Flatpak-4A90D9?logo=flatpak&amp;logoColor=white&amp;style=flat" alt="Flatpak"></a>
</p>

**Ваша Яндекс Музыка — в нативном приложении для Linux.**

Любимые треки, библиотека и «Моя волна» в одном приложении на GTK4 и Libadwaita.
Интерфейс на русском, привычное управление средствами Linux.

- Библиотека, понравившиеся композиции и личные плейлисты.
- Поиск треков, альбомов, исполнителей и плейлистов.
- Воспроизведение, очередь, перемешивание и повтор.
- «Моя волна», лайки и управление своими плейлистами.
- Информация о треке, похожие композиции и тексты; синхронизация через LRCLIB с вашего согласия.
- Медиаклавиши, MPRIS, системный медиавиджет, светлая и тёмная темы.
- Кеш музыки и обложек для повторного использования при актуальном доступе к сервису.

## Установка

Доступны две сборки для Linux x86_64.

- **[Скачать Flatpak](https://github.com/tracerxbrhd/karin/releases/latest)** — рекомендуемый вариант. Откройте файл `.flatpak` в менеджере приложений или установите его командой `flatpak install --user ./имя-файла.flatpak`. Установщик подключает подписанный канал обновлений Karin.
- **[Скачать Native](https://github.com/tracerxbrhd/karin/releases/latest)** — распакуйте архив `linux-x86_64.tar.gz` и запустите `./run-karin`. [Системные зависимости и сборка](docs/BUILDING.md#fedora-prerequisites)

Для 0.8.1 доступны инструкции в [заметках этого выпуска](https://github.com/tracerxbrhd/karin/releases/tag/v0.8.1). Ссылка на полные исходники каждой сборки находится в заметках к её выпуску.

Для музыки нужен доступ к Яндекс Музыке. Доступность композиций и текстов зависит
от сервиса, аккаунта и подписки. Полноценный автономный режим не поддерживается.

## О проекте

Karin — независимый неофициальный клиент, не связанный с Яндексом.
Условия использования OAuth/API и публичного распространения интеграции ещё уточняются.

Код — [GNU GPL-3.0-or-later](LICENSE).
[Сторонние лицензии и благодарности](docs/LICENSING.md).

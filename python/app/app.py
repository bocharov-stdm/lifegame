# app.py — окно и переключение экранов.
#
# frame(events, dt) — ровно один кадр: события, время, отрисовка. Главный
# цикл run() только крутит его, поэтому тесты гоняют приложение по кадрам
# синтетическими событиями, без окна и без бесконечного цикла.

import pygame

from . import settings as settings_io
from . import theme
from .render   import blit_text
from .session  import Session
from .settings import SETTINGS_PATH, Settings
from .scenes        import Backdrop
from .scenes.game   import GameScene
from .scenes.help   import HelpScene
from .scenes.menu   import MenuScene
from .scenes.prefs  import PrefsScene
from .scenes.setup  import SetupScene
from .theme    import FAINT, S

MAX_DT = 0.1        # после зависания окна не пытаемся «догнать» пропущенное время


class App:
    def __init__(self, size=None, settings_path=SETTINGS_PATH, headless=False):
        """headless — рисовать в память, без окна (для тестов);
        settings_path=None — не читать и не писать файл настроек."""
        self.headless = headless
        self.settings_path = settings_path
        self.settings = settings_io.load(settings_path) if settings_path else Settings()
        self.system_scale = 1.0 if headless else theme.enable_dpi_awareness()

        if headless:
            pygame.font.init()
        else:
            pygame.init()
            pygame.display.set_caption("Tiny Life")
            pygame.display.set_icon(theme.window_icon())
            pygame.key.set_repeat(350, 40)
        self.windowed_size = size or theme.initial_window_size(self.system_scale)

        self.mouse = (0, 0)
        self.fps = 0.0
        self.running = True
        self.effective_scale = 1.0
        self.session = None          # идущая партия, если есть
        self.game = None             # её экран
        self.previous = None         # куда возвращает «Назад»

        self.screen = None
        self._apply_display()
        self.backdrop = Backdrop(self)
        self.scene = MenuScene(self)
        self.layout()

    # ── окно и масштаб ──────────────────────────────────────────────────────
    def _apply_display(self):
        if self.headless:
            self.screen = pygame.Surface(self.windowed_size)
        elif self.settings.fullscreen:
            self.screen = pygame.display.set_mode((0, 0), pygame.FULLSCREEN)
        else:
            self.screen = pygame.display.set_mode(self.windowed_size, pygame.RESIZABLE)

    def resize(self, size):
        """Новый размер окна (в тестах — новый холст)."""
        self.windowed_size = size
        if self.headless:
            self.screen = pygame.Surface(size)
        else:
            self.screen = pygame.display.get_surface()
        self.layout()

    def set_fullscreen(self, value):
        self.settings.fullscreen = bool(value)
        if not self.headless:
            self._apply_display()
        self.layout()

    def ui_scale(self):
        """Масштаб из настроек (или системный), но не крупнее, чем влезает в окно."""
        wanted = self.settings.ui_scale or self.system_scale
        w, h = self.screen.get_size()
        return max(0.6, min(wanted, w / theme.MIN_W, h / theme.MIN_H))

    def layout(self):
        self.effective_scale = self.ui_scale()
        theme.set_scale(self.effective_scale)
        size = self.screen.get_size()
        self.backdrop.layout(size)
        for scene in {id(s): s for s in (self.scene, self.previous, self.game) if s}.values():
            scene.layout(size)

    # ── переходы ────────────────────────────────────────────────────────────
    def go(self, scene, remember=True):
        if remember and scene is not self.scene:
            self.previous = self.scene
        self.scene = scene
        scene.layout(self.screen.get_size())

    def back(self):
        self.go(self.previous or MenuScene(self), remember=False)
        self.previous = None

    def open_menu(self):
        self.go(MenuScene(self), remember=False)

    def open_setup(self):
        self.go(SetupScene(self))

    def open_prefs(self):
        self.go(PrefsScene(self))

    def open_help(self):
        self.go(HelpScene(self))

    def start_game(self):
        if self.settings.random_seed:
            self.settings.roll_seed()
        self.save_settings()
        self._play(Session(self.settings))

    def restart_game(self):
        if self.session is not None:
            self._play(self.session.restart())

    def continue_game(self):
        if self.game is not None:
            if self.game.overlay is not None and self.game.overlay.kind == "pause":
                self.game.close_overlay()          # «Продолжить» — значит играть
            self.go(self.game, remember=False)

    def _play(self, session):
        self.session = session
        self.game = GameScene(self, session)
        self.previous = None
        self.go(self.game, remember=False)

    def quit(self):
        self.running = False

    def save_settings(self):
        if self.settings_path:
            settings_io.save(self.settings, self.settings_path)

    # ── кадр ────────────────────────────────────────────────────────────────
    def frame(self, events, dt):
        if not self.headless:
            self.mouse = pygame.mouse.get_pos()
        for event in events:
            if event.type in (pygame.MOUSEMOTION, pygame.MOUSEBUTTONDOWN,
                              pygame.MOUSEBUTTONUP):
                self.mouse = event.pos
            if event.type == pygame.QUIT:
                self.quit()
            elif event.type == pygame.VIDEORESIZE:
                if not self.settings.fullscreen:
                    self.resize(event.size)
            elif event.type == pygame.KEYDOWN and event.key == pygame.K_F11:
                self.set_fullscreen(not self.settings.fullscreen)
            else:
                self.scene.handle(event)

        self.scene.update(min(dt, MAX_DT))
        self.scene.draw(self.screen)
        if self.settings.show_fps:
            h = self.screen.get_height()
            blit_text(self.screen, "tiny", f"{self.fps:.0f} FPS", FAINT, "bottomleft",
                      bottomleft=(S(6), h - S(4)))
        if not self.headless:
            pygame.display.flip()

    def run(self):
        clock = pygame.time.Clock()
        try:
            while self.running:
                dt = clock.tick(theme.FRAME_RATE) / 1000
                self.fps = clock.get_fps()
                self.frame(pygame.event.get(), dt)
        finally:
            self.save_settings()
            pygame.quit()

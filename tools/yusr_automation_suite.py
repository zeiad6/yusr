#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
========================================================================================
منظومة الفحص الجنائي والأتمتة المتقدمة لتطبيق "يُسر" (Yusr v0.2.0 / v0.2.1)
المبني باستخدام Tauri v2 + Microsoft Edge WebView2 + React 18 + Axum/Tokio
========================================================================================
المؤلف: خبير جودة وأتمتة النظم (Senior QA Automation & DevOps Engineer)
الوصف: سكربت غير متزامن فائق القوة (Resilient & Production-Ready) للتحكم الصامت
       بالتطبيق عبر بروتوكول Chrome DevTools Protocol (CDP) على المنفذ 9222،
       مع دعم التشغيل التلقائي للتطبيق، واختبار تسجيل الدخول، واستكشاف القوائم،
       وفحص التدقيق الجنائي للدفاتر والمستشار الذكي، وتوليد تقارير شاملة مع لقطات شاشة.
========================================================================================
"""

import asyncio
import os
import sys
import json
import time
import socket
import logging
import platform
import subprocess
from pathlib import Path
from dataclasses import dataclass, field
from typing import List, Dict, Optional, Any
from urllib.request import urlopen, Request
from urllib.error import URLError

from playwright.async_api import (
    async_playwright,
    Playwright,
    Browser,
    BrowserContext,
    Page,
    Locator,
    TimeoutError as PlaywrightTimeoutError,
    Error as PlaywrightError
)

# --------------------------------------------------------------------------------------
# 1. إعدادات السجلات والتهيئة العامة (Configuration & Logging)
# --------------------------------------------------------------------------------------

RESULTS_DIR = Path("yusr_automation_results")
SCREENSHOTS_DIR = RESULTS_DIR / "screenshots"

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s",
    handlers=[
        logging.StreamHandler(sys.stdout)
    ]
)
logger = logging.getLogger("YusrQA")


@dataclass
class SectionAuditResult:
    """بيانات نتيجة فحص قسم منفرد في التطبيق"""
    name: str
    path: str
    status: str  # "SUCCESS", "FAILED", "WARNING", "SKIPPED"
    duration_sec: float
    screenshot_path: Optional[str] = None
    error_details: Optional[str] = None


@dataclass
class ForensicReportData:
    """التقرير الجنائي المتكامل لحالة التطبيق والواجهة"""
    start_time: str = ""
    end_time: str = ""
    app_version: str = "v0.2.1"
    cdp_endpoint: str = "http://localhost:9222"
    connected_successfully: bool = False
    login_success: bool = False
    boot_error: Optional[str] = None
    sections: List[SectionAuditResult] = field(default_factory=list)
    console_errors: List[Dict[str, Any]] = field(default_factory=list)
    console_warnings: List[Dict[str, Any]] = field(default_factory=list)
    failed_requests: List[Dict[str, Any]] = field(default_factory=list)
    advanced_checks: Dict[str, Any] = field(default_factory=dict)


# --------------------------------------------------------------------------------------
# 2. فئة إدارة دورة حياة التطبيق والاتصال عبر CDP (App Lifecycle & CDP Controller)
# --------------------------------------------------------------------------------------

class YusrAppLauncher:
    """
    مسؤولة عن إدارة تشغيل التطبيق والتحقق من فتح منفذ تصحيح الأخطاء البرمجي (CDP Port 9222).
    """

    DEFAULT_PORT = 9222
    CDP_URL = f"http://localhost:{DEFAULT_PORT}"

    @staticmethod
    def is_port_open(port: int = DEFAULT_PORT, host: str = "127.0.0.1") -> bool:
        """فحص سريع لمقبس الشبكة لمعرفة هل المنفذ يستمع للاتصالات."""
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
            sock.settimeout(1.0)
            result = sock.connect_ex((host, port))
            return result == 0

    @classmethod
    def is_cdp_ready(cls) -> bool:
        """التحقق من جاهزية نقطة نهاية Chrome DevTools Protocol."""
        if not cls.is_port_open(cls.DEFAULT_PORT):
            return False
        try:
            req = Request(f"{cls.CDP_URL}/json/version", headers={"User-Agent": "YusrQA-Agent"})
            with urlopen(req, timeout=2.0) as resp:
                return resp.status == 200
        except (URLError, Exception):
            return False

    @staticmethod
    def find_executable_paths() -> List[Path]:
        """
        البحث الذكي عن مسارات ملف تشغيل تطبيق يُسر (Yusr.exe) في ويندوز ومسارات التطوير.
        """
        candidates: List[Path] = []

        # 1. متغير بيئة مخصص
        env_custom = os.getenv("YUSR_EXE_PATH")
        if env_custom:
            candidates.append(Path(env_custom))

        # 2. مسارات تثبيت حزم ويندوز الافتراضية (MSI / Inno Setup)
        local_app_data = os.getenv("LOCALAPPDATA", "")
        program_files = os.getenv("ProgramFiles", "")
        program_files_x86 = os.getenv("ProgramFiles(x86)", "")

        if local_app_data:
            candidates.append(Path(local_app_data) / "Programs" / "Yusr" / "Yusr.exe")
            candidates.append(Path(local_app_data) / "Yusr" / "Yusr.exe")

        if program_files:
            candidates.append(Path(program_files) / "Yusr" / "Yusr.exe")
            candidates.append(Path(program_files) / "ZeidexSoft" / "Yusr" / "Yusr.exe")

        if program_files_x86:
            candidates.append(Path(program_files_x86) / "Yusr" / "Yusr.exe")

        # 3. مسارات مستودع التطوير الموضعي (Local Workspace Build Targets)
        cwd = Path.cwd()
        candidates.append(cwd / "app" / "src-tauri" / "target" / "release" / "Yusr.exe")
        candidates.append(cwd / "target" / "release" / "Yusr.exe")
        candidates.append(cwd / "Yusr.exe")
        candidates.append(cwd / "app" / "src-tauri" / "target" / "release" / "yusr")
        candidates.append(cwd / "target" / "release" / "yusr")

        return [p for p in candidates if p.exists() and p.is_file()]

    @classmethod
    async def ensure_app_running(cls, max_wait_sec: int = 30) -> bool:
        """
        التحقق من عمل التطبيق، وإذا لم يكن يعمل يتم إطلاقه برمجياً مع تمرير
        متغير البيئة الخاص بـ WebView2 لتفعيل منفذ التصحيح 9222.
        """
        if cls.is_cdp_ready():
            logger.info("✓ تم اكتشاف جلسة CDP نشطة مسبقاً على المنفذ 9222.")
            return True

        logger.warning("! لم يتم العثور على جلسة CDP نشطة على المنفذ 9222. جارٍ البحث عن التطبيق وتشغيله...")
        found_executables = cls.find_executable_paths()

        if not found_executables:
            logger.error("❌ لم يتم العثور على ملف تشغيل التطبيق في المسارات الشائعة.")
            logger.info("نصيحة: يمكنك تشغيل التطبيق يدوياً بعد ضبط الأمر:")
            logger.info('  set WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222"')
            return False

        target_exe = found_executables[0]
        logger.info(f"🚀 تشغيل التطبيق تلقائياً من المسار: {target_exe}")

        # في ويندوز: إذا لم يكن منفذ 9222 مفتوحاً، ننهي أي نسخة سابقة لأن بقاء نسخة سابقة
        # يمنع WebView2 من قراءة متغير البيئة WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS
        if platform.system() == "Windows" and not cls.is_port_open(cls.DEFAULT_PORT):
            logger.info("🧹 إنهاء أي نسخ سابقة لتطبيق يُسر لمنع تعارض WebView2 وتفعيل منفذ التصحيح...")
            try:
                subprocess.run(
                    ["taskkill", "/F", "/IM", "Yusr.exe", "/IM", "yusr.exe"],
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    check=False
                )
                await asyncio.sleep(1.0)
            except Exception as e:
                logger.debug(f"تنبيه أثناء محاولة إنهاء النسخ السابقة عبر taskkill: {e}")

        # تجهيز متغيرات البيئة الخاصة بـ WebView2
        custom_env = os.environ.copy()
        custom_env["WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS"] = f"--remote-debugging-port={cls.DEFAULT_PORT}"

        # تشغيل التطبيق كعملية مستقلة في الخلفية (Detached Process)
        try:
            creation_flags = 0
            if platform.system() == "Windows":
                # DETACHED_PROCESS = 0x00000008, CREATE_NO_WINDOW = 0x08000000
                creation_flags = subprocess.DETACHED_PROCESS | subprocess.CREATE_NO_WINDOW

            subprocess.Popen(
                [str(target_exe)],
                env=custom_env,
                creationflags=creation_flags,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                stdin=subprocess.DEVNULL
            )
        except Exception as e:
            logger.error(f"❌ فشل تشغيل التطبيق تلقائياً: {e}")
            return False

        # انتظار إتاحة منفذ CDP عبر التكرار والتأخير التدريجي
        logger.info("⏳ بانتظار استجابة منفذ التصحيح CDP...")
        start_time = time.time()
        while time.time() - start_time < max_wait_sec:
            if cls.is_cdp_ready():
                logger.info("✓ نجح الاتصال بمنفذ التصحيح CDP بنجاح.")
                return True
            await asyncio.sleep(1.0)

        logger.error(f"❌ انتهت مهلة الانتظار ({max_wait_sec} ثانية) دون استجابة منفذ CDP.")
        return False


# --------------------------------------------------------------------------------------
# 3. دوال التفاعل البرمجي الصامت مع الـ DOM (Silent DOM Automation Helpers)
# --------------------------------------------------------------------------------------

class DOMDriver:
    """
    أدوات التفاعل البرمجي مع عناصر الـ DOM دون استخدام مؤشر الفأرة الحقيقي
    ودون اشتراط التركيز على النافذة (Headless / Minimized execution friendly).
    """

    @staticmethod
    async def dispatch_click(page: Page, selector: str, timeout_ms: int = 4000) -> bool:
        """
        إرسال حدث النقر (Synthetic Click Event) مباشرة للعنصر داخل شجرة الـ DOM.
        يعمل حتى إذا كانت النافذة مصغرة أو مخفية في شريط المهام.
        """
        try:
            locator = page.locator(selector).first
            await locator.wait_for(state="attached", timeout=timeout_ms)
            # نستخدم dispatch_event لضمان عدم تحريك الفأرة أو الحاجة لتركيز الشاشة
            await locator.dispatch_event("click")
            return True
        except Exception:
            # Fallback باستخدام JavaScript المباشر
            try:
                clicked = await page.evaluate(f"""
                    () => {{
                        const el = document.querySelector("{selector}");
                        if (el) {{
                            el.dispatchEvent(new MouseEvent('click', {{
                                bubbles: true,
                                cancelable: true,
                                view: window
                            }}));
                            return true;
                        }}
                        return false;
                    }}
                """)
                return bool(clicked)
            except Exception:
                return False

    @staticmethod
    async def inject_react_value(page: Page, selector: str, value: str) -> bool:
        """
        حقن القيمة النصية مباشرة في حقول إدخال React 18 مع إطلاق أحداث
        `input` و `change` المتوافقة مع الـ Synthetic Events لنظام React.
        """
        try:
            locator = page.locator(selector).first
            await locator.wait_for(state="attached", timeout=4000)
            await page.evaluate(f"""
                (val) => {{
                    const el = document.querySelector("{selector}");
                    if (!el) return false;
                    // تعديل القيمة عبر الـ Prototype الأصلي لتجاوز قيود React Controlled Component
                    const proto = el instanceof HTMLInputElement 
                        ? window.HTMLInputElement.prototype 
                        : window.HTMLTextAreaElement.prototype;
                    const setter = Object.getOwnPropertyDescriptor(proto, 'value').set;
                    setter.call(el, val);
                    
                    // إطلاق الأحداث المطلوبة لتحديث State الكومبوننت في React
                    el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                    el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                    return true;
                }}
            """, value)
            return True
        except Exception:
            try:
                await page.locator(selector).first.fill(value, timeout=2000)
                return True
            except Exception:
                return False


# --------------------------------------------------------------------------------------
# 4. محرك الفحص الجنائي والأتمتة الرئيسي (Yusr Automation Suite Engine)
# --------------------------------------------------------------------------------------

class YusrAutomationSuite:
    """
    المحرك الشامل لتنفيذ سيناريوهات الاختبار، استخراج البيانات، والتوثيق الجنائي.
    """

    def __init__(self, cdp_url: str = "http://localhost:9222"):
        self.cdp_url = cdp_url
        self.report = ForensicReportData(cdp_endpoint=cdp_url)
        self.playwright: Optional[Playwright] = None
        self.browser: Optional[Browser] = None
        self.context: Optional[BrowserContext] = None
        self.page: Optional[Page] = None

        # إنشاء المجلدات اللازمة للنتائج
        RESULTS_DIR.mkdir(parents=True, exist_ok=True)
        SCREENSHOTS_DIR.mkdir(parents=True, exist_ok=True)

    # ----------------------------------------------------------------------------------
    # إدارة الجلسة والاستماع للأحداث (Session Setup & Event Hooks)
    # ----------------------------------------------------------------------------------

    async def check_database_boot_error(self) -> bool:
        """
        فحص أولي للتحقق من عدم وجود شاشة خطأ قاعدة البيانات (.boot-error الخاصة بـ initdb.exe).
        يرجع True إذا كانت الشاشة سليمة، وFalse في حال ظهور خطأ الإقلاع مع توثيقه في التقرير الجنائي.
        """
        if not self.page:
            return True

        try:
            try:
                await self.page.wait_for_load_state("domcontentloaded", timeout=3000)
            except Exception:
                pass

            boot_error_elem = self.page.locator(".boot-error")
            if await boot_error_elem.count() > 0 and await boot_error_elem.first.is_visible():
                error_text = (await boot_error_elem.first.inner_text()).strip()
                logger.error(f"❌ تم اكتشاف شاشة خطأ إقلاع قاعدة البيانات (.boot-error الخاصة بـ initdb.exe): {error_text}")
                shot = await self.capture_silent_screenshot("00_database_boot_error")
                self.report.boot_error = error_text or "Database boot error (.boot-error)"
                self.report.advanced_checks["database_boot"] = {
                    "status": "FAILED",
                    "error": self.report.boot_error,
                    "screenshot": shot
                }
                return False

            return True
        except Exception as e:
            logger.warning(f"تنبيه أثناء فحص خطأ إقلاع قاعدة البيانات: {e}")
            return True

    async def setup_connection(self) -> bool:
        """الاتصال بـ WebView2 عبر بروتوكول CDP وإعداد مراقبة الأحداث والفحص الأولي."""
        try:
            self.playwright = await async_playwright().start()
            logger.info(f"🔌 جارٍ الاتصال بـ CDP على {self.cdp_url} ...")
            self.browser = await self.playwright.chromium.connect_over_cdp(self.cdp_url)

            contexts = self.browser.contexts
            if not contexts:
                logger.error("❌ لم يتم العثور على سياقات متصفح (Browser Contexts) في WebView2.")
                return False

            self.context = contexts[0]
            pages = self.context.pages
            if not pages:
                logger.error("❌ لم يتم العثور على صفحات مفتوحة في التطبيق.")
                return False

            # اختيار الصفحة الرئيسية للتطبيق
            self.page = pages[0]
            self.report.connected_successfully = True
            logger.info(f"✓ تم الاتصال بنافذة التطبيق بنجاح. العنوان الحالي: {await self.page.title()}")

            # تفعيل الاستماع لأخطاء الـ Console وشبكة الاتصال
            self._attach_telemetry_listeners()

            # فحص أولي للتحقق من عدم وجود شاشة خطأ قاعدة البيانات (.boot-error الخاصة بـ initdb.exe) قبل البدء باختبار تسجيل الدخول
            is_boot_clean = await self.check_database_boot_error()
            if not is_boot_clean:
                logger.error("❌ فشل الفحص الأولي: تم اكتشاف شاشة خطأ قاعدة البيانات (.boot-error) الناتجة عن تعثر initdb.exe.")
                return False

            logger.info("✓ الفحص الأولي: شاشة التطبيق سليمة وخالية من أخطاء إقلاع قاعدة البيانات (.boot-error).")
            return True

        except Exception as e:
            logger.error(f"❌ تعذر الاتصال بـ CDP: {e}")
            return False

    def _attach_telemetry_listeners(self):
        """تسجيل مستمعي أحداث الـ Console والشبكة لرصد أي انهيارات أو أخطاء فوراً."""
        if not self.page:
            return

        def on_console(msg):
            entry = {
                "type": msg.type,
                "text": msg.text,
                "location": msg.location,
                "timestamp": time.strftime("%Y-%m-%d %H:%M:%S")
            }
            if msg.type == "error":
                self.report.console_errors.append(entry)
                logger.warning(f"  [Console Error] {msg.text}")
            elif msg.type == "warning":
                self.report.console_warnings.append(entry)

        def on_request_failed(request):
            failure = request.failure
            failure_text = failure if isinstance(failure, str) else str(failure)
            entry = {
                "url": request.url,
                "method": request.method,
                "failure": failure_text,
                "timestamp": time.strftime("%Y-%m-%d %H:%M:%S")
            }
            self.report.failed_requests.append(entry)
            logger.warning(f"  [Network Failed] {request.method} {request.url} -> {failure_text}")

        self.page.on("console", on_console)
        self.page.on("requestfailed", on_request_failed)

    # ----------------------------------------------------------------------------------
    # مساعدة التقاط الصور التوثيقية (Silent High-Res Screenshots)
    # ----------------------------------------------------------------------------------

    async def capture_silent_screenshot(self, name: str) -> Optional[str]:
        """التقاط لقطة شاشة صامتة وحفظها بجودة عالية دون تحريك النافذة."""
        if not self.page:
            return None
        safe_name = "".join(c if c.isalnum() or c in ("-", "_") else "_" for c in name)
        filename = f"{safe_name}.png"
        target_path = SCREENSHOTS_DIR / filename
        try:
            await self.page.screenshot(path=str(target_path), full_page=False)
            return str(target_path)
        except Exception as e:
            logger.warning(f"تعذر التقاط لقطة الشاشة لـ {name}: {e}")
            return None

    # ----------------------------------------------------------------------------------
    # السيناريو 1: فحص واختبار شاشة تسجيل الدخول الذكي (Smart Login Test)
    # ----------------------------------------------------------------------------------

    async def execute_smart_login(self) -> bool:
        """
        فحص شاشة الدخول:
        1. البحث عن زر التعبئة التلقائية والضغط عليه برمجياً.
        2. كبديل: حقن بيانات admin / admin12345 في الحقول مباشرة.
        3. إرسال النموذج والانتظار حتى الانتقال للوحة التحكم.
        """
        logger.info("\n--- [السيناريو 1]: فحص شاشة تسجيل الدخول الذكي ---")
        page = self.page
        if not page:
            return False

        # التأكد من استقرار الصفحة الحالية
        try:
            await page.wait_for_load_state("domcontentloaded", timeout=5000)
        except Exception:
            pass

        # التحقق من عدم وجود شاشة خطأ قاعدة البيانات (.boot-error) قبل البدء باختبار تسجيل الدخول
        if not await self.check_database_boot_error():
            logger.error("❌ لا يمكن البدء باختبار تسجيل الدخول لوجود شاشة خطأ قاعدة البيانات (.boot-error).")
            return False

        # التحقق هل المستخدم مسجل الدخول بالفعل
        current_url = page.url
        has_nav = await page.locator("nav.sidebar-nav").count() > 0
        if has_nav and "/login" not in current_url:
            logger.info("✓ المستخدم مسجل دخوله مسبقاً داخل النظام.")
            self.report.login_success = True
            await self.capture_silent_screenshot("00_already_logged_in")
            return True

        # توثيق شاشة تسجيل الدخول قبل المحاولة
        await self.capture_silent_screenshot("01_login_screen_initial")

        # الخطوة 1: محاولة الضغط على زر التعبئة التلقائية (Auto-fill)
        autofill_clicked = False
        autofill_selectors = [
            "button.btn-text-action",
            "button:has-text('تعبئة')",
            "button[title*='تعبئة']",
            ".login-creds-header button"
        ]

        for sel in autofill_selectors:
            if await page.locator(sel).count() > 0:
                logger.info(f"✓ تم العثور على زر التعبئة التلقائية عبر المحدد: {sel}")
                autofill_clicked = await DOMDriver.dispatch_click(page, sel)
                if autofill_clicked:
                    logger.info("✓ تم تفعيل زر التعبئة التلقائية برمجياً بنجاح.")
                    await asyncio.sleep(0.5)
                    break

        # التحقق مما إذا كانت الحقول قد تم ملؤها
        user_val = await page.locator("#username").input_value() if await page.locator("#username").count() > 0 else ""
        pass_val = await page.locator("#password").input_value() if await page.locator("#password").count() > 0 else ""

        # الخطوة 2: الخطة البديلة (Fallback Injection) في حال عدم اكتمال التعبئة
        if not user_val or not pass_val:
            logger.info("! استخدام الخطة البديلة: حقن اسم المستخدم وكلمة المرور برمجياً...")
            await DOMDriver.inject_react_value(page, "#username", "admin")
            await DOMDriver.inject_react_value(page, "#password", "admin12345")
            await asyncio.sleep(0.3)

        # التقاط صورة لنموذج الدخول بعد ملء البيانات
        await self.capture_silent_screenshot("02_login_form_filled")

        # الخطوة 3: الضغط على زر تسجيل الدخول
        submit_selectors = [
            "button[type='submit']",
            ".login-form button.btn-primary",
            "button:has-text('دخول')",
            "button:has-text('تسجيل الدخول')"
        ]

        submitted = False
        for s_sel in submit_selectors:
            if await page.locator(s_sel).count() > 0:
                logger.info(f"✓ إرسال نموذج الدخول بالضغط على الزر: {s_sel}")
                submitted = await DOMDriver.dispatch_click(page, s_sel)
                if submitted:
                    break

        if not submitted:
            logger.error("❌ تعذر تحديد زر إرسال تسجيل الدخول.")
            return False

        # انتظار نجاح الدخول وظهور القائمة الجانبية أو استقرار الشبكة
        try:
            await page.wait_for_selector("nav.sidebar-nav", state="attached", timeout=10000)
            await page.wait_for_load_state("networkidle", timeout=5000)
            logger.info("✓ تم تسجيل الدخول بنجاح والوصول إلى لوحة التحكم.")
            self.report.login_success = True
            await self.capture_silent_screenshot("03_dashboard_post_login")
            return True
        except Exception as e:
            logger.error(f"❌ لم تظهر لوحة التحكم بعد محاولة الدخول: {e}")
            await self.capture_silent_screenshot("03_login_failure")
            return False

    # ----------------------------------------------------------------------------------
    # السيناريو 2: الفحص التلقائي والديناميكي للقائمة الجانبية (Sidebar Exploration)
    # ----------------------------------------------------------------------------------

    async def explore_sidebar_sections(self):
        """
        استخراج روابط وأزرار القائمة الجانبية ديناميكياً مع استبعاد تسجيل الخروج،
        وتصفح كل قسم بشكل معزول مع توثيقه بصرياً.
        """
        logger.info("\n--- [السيناريو 2]: الفحص التلقائي والديناميكي للقائمة الجانبية ---")
        page = self.page
        if not page:
            return

        # استخراج كافة الروابط في القائمة الجانبية
        nav_elements = await page.query_selector_all("nav.sidebar-nav a.nav-item")
        if not nav_elements:
            # محددات إضافية مرنة في حال اختلاف الـ Classes
            nav_elements = await page.query_selector_all(".sidebar a, nav a")

        logger.info(f"تم اكتشاف {len(nav_elements)} عنصر ملاحة في القائمة الجانبية.")

        discovered_items: List[Dict[str, str]] = []
        for el in nav_elements:
            href = await el.get_attribute("href") or ""
            text = (await el.inner_text()).strip()
            aria_label = await el.get_attribute("aria-label") or ""
            label = text or aria_label or href

            # استبعاد زر تسجيل الخروج أو أزرار العمليات غير الآمنة
            if any(term in label for term in ["تسجيل الخروج", "خروج", "Logout", "signout"]):
                continue

            discovered_items.append({
                "label": label,
                "href": href,
                "selector": f"nav.sidebar-nav a[href='{href}']" if href else f"text={label}"
            })

        logger.info(f"سيتم فحص {len(discovered_items)} قسماً معتمداً في القائمة...")

        # تصفح كل قسم على حدة مع عزل الأخطاء (Try-Except per section)
        for idx, item in enumerate(discovered_items, start=1):
            label = item["label"]
            href = item["href"]
            t0 = time.time()
            logger.info(f"\n[{idx}/{len(discovered_items)}] فحص القسم: {label} ({href})")

            try:
                # محاولة الضغط برمجياً عبر الـ DOM
                clicked = await DOMDriver.dispatch_click(page, item["selector"], timeout_ms=3000)
                if not clicked:
                    # البديل عبر النقر على الرابط مباشرة بالـ evaluate
                    await page.evaluate(f"""
                        (targetHref) => {{
                            const link = Array.from(document.querySelectorAll('a')).find(a => a.getAttribute('href') === targetHref);
                            if (link) link.dispatchEvent(new MouseEvent('click', {{ bubbles: true, cancelable: true }}));
                        }}
                    """, href)

                # انتظار استقرار الواجهة بعد الانتقال
                await asyncio.sleep(0.8)
                try:
                    await page.wait_for_load_state("domcontentloaded", timeout=4000)
                except Exception:
                    pass

                # التحقق من خلو الواجهة من شاشات الخطأ الكارثية
                has_crash = await page.locator(".boot-error, .error-boundary, [role='alert'].fatal").count() > 0
                status = "FAILED" if has_crash else "SUCCESS"
                err_msg = "اكتشاف شاشة خطأ حرجة في واجهة القسم" if has_crash else None

                # التقاط صورة توثيقية للقسم
                shot_name = f"nav_{idx:02d}_{label.replace(' ', '_')}"
                shot_file = await self.capture_silent_screenshot(shot_name)
                duration = round(time.time() - t0, 2)

                self.report.sections.append(SectionAuditResult(
                    name=label,
                    path=href,
                    status=status,
                    duration_sec=duration,
                    screenshot_path=shot_file,
                    error_details=err_msg
                ))

                logger.info(f"  ✓ اكتمل فحص {label} بنجاح خلال {duration}s [الحالة: {status}]")

            except Exception as ex:
                duration = round(time.time() - t0, 2)
                err_details = str(ex)
                logger.error(f"  ❌ تعثر فحص القسم {label}: {err_details}")
                shot_file = await self.capture_silent_screenshot(f"nav_err_{idx:02d}_{label}")

                self.report.sections.append(SectionAuditResult(
                    name=label,
                    path=href,
                    status="FAILED",
                    duration_sec=duration,
                    screenshot_path=shot_file,
                    error_details=err_details
                ))
                # يستمر السكربت للقسم التالي دون توقف

    # ----------------------------------------------------------------------------------
    # السيناريو 3: فحص العمليات والميزات المتقدمة للتطبيق (Advanced Features)
    # ----------------------------------------------------------------------------------

    async def execute_advanced_checks(self):
        """
        إجراء فحوصات دقيقة للمسارات الحساسة:
        1. التدقيق الجنائي للـ Ledger (/audit) واختبار التحقق من السلسلة التشفيرية.
        2. بوابة التحقق من التقارير (/verify/report).
        3. فحص إعدادات المستشار الذكي المحلي (llama.cpp / Qwen).
        4. محاكاة تعبئة نموذج إضافة تبرع / قيد محاسبي تجريبي.
        """
        logger.info("\n--- [السيناريو 3]: فحص العمليات والميزات المتقدمة للتطبيق ---")
        page = self.page
        if not page:
            return

        # ------------------------------------------------------------------------------
        # 1. فحص مسار التدقيق الجنائي للدفاتر (/audit)
        # ------------------------------------------------------------------------------
        logger.info("\n[1/4] فحص مسار التدقيق الجنائي للـ Ledger (/audit)...")
        try:
            await DOMDriver.dispatch_click(page, "a[href='/audit']")
            await asyncio.sleep(1.0)

            # التحقق من وجود زر "التحقق" (Cryptographic Chain Verification)
            verify_btn_selector = "button.btn-primary:has-text('تحقق'), button:has(svg.lucide-shield-check)"
            has_verify = await page.locator(verify_btn_selector).count() > 0

            audit_details = {"page_loaded": True, "chain_verified": False, "head_hash": None}

            if has_verify:
                logger.info("  ✓ تم العثور على زر فحص السلسلة التشفيرية، جارٍ تفعيله برمجياً...")
                await DOMDriver.dispatch_click(page, verify_btn_selector)
                await asyncio.sleep(1.5)

                # قراءة النتيجة من الـ DOM
                result_box = page.locator(".settings-note, .settings-warning, [role='status']").first
                if await result_box.count() > 0:
                    text_result = await result_box.inner_text()
                    audit_details["chain_result_text"] = text_result.strip()
                    audit_details["chain_verified"] = "سليمة" in text_result or "ok" in text_result.lower()
                    logger.info(f"  ✓ نتيجة فحص السلسلة: {text_result.strip()}")

            shot = await self.capture_silent_screenshot("adv_01_audit_ledger_verified")
            audit_details["screenshot"] = shot
            self.report.advanced_checks["audit_ledger"] = audit_details

        except Exception as e:
            logger.error(f"  ❌ خطأ أثناء فحص مسار التدقيق: {e}")
            self.report.advanced_checks["audit_ledger"] = {"error": str(e)}

        # ------------------------------------------------------------------------------
        # 2. فحص مسار التحقق من التقارير (/verify/report)
        # ------------------------------------------------------------------------------
        logger.info("\n[2/4] فحص بوابة التحقق من التقارير المنشورة (/verify/report)...")
        try:
            # الانتقال البرمجي باستخدام pushState و PopStateEvent ليتوافق مع BrowserRouter في React 18 دون كسر الجلسة
            await page.evaluate("""() => {
                window.history.pushState({}, '', '/verify/report');
                window.dispatchEvent(new PopStateEvent('popstate'));
            }""")
            await asyncio.sleep(1.0)

            has_verify_input = await page.locator("input, [placeholder*='رمز'], [placeholder*='Token']").count() > 0
            shot = await self.capture_silent_screenshot("adv_02_verify_report_portal")

            self.report.advanced_checks["verify_report"] = {
                "accessible": True,
                "input_found": has_verify_input,
                "screenshot": shot
            }
            logger.info(f"  ✓ تم التحقق من بوابة التقارير. وجود حقل الإدخال: {has_verify_input}")

            # العودة إلى المسار الرئيسي عبر BrowserRouter للحفاظ على شريط التنقل للفحوصات اللاحقة
            await page.evaluate("""() => {
                window.history.pushState({}, '', '/');
                window.dispatchEvent(new PopStateEvent('popstate'));
            }""")
            await asyncio.sleep(0.5)

        except Exception as e:
            logger.warning(f"  ! تنبيه في مسار بوابة التقارير: {e}")
            self.report.advanced_checks["verify_report"] = {"error": str(e)}

        # ------------------------------------------------------------------------------
        # 3. فحص صفحة الإعدادات ← المستشار الذكي المحلي (Local AI Advisor)
        # ------------------------------------------------------------------------------
        logger.info("\n[3/4] فحص حالة المستشار الذكي المحلي في صفحة الإعدادات (/settings)...")
        try:
            await DOMDriver.dispatch_click(page, "a[href='/settings']")
            await asyncio.sleep(1.2)

            advisor_info: Dict[str, Any] = {"section_found": False}

            # البحث عن عناصر قسم المستشار الذكي
            advisor_section = page.locator("section:has-text('المستشار'), form:has-text('المستشار'), .local-model")
            if await advisor_section.count() > 0:
                advisor_info["section_found"] = True
                # فحص الأزرار المتاحة (تنزيل النموذج، التشغيل، الإيقاف)
                buttons = await advisor_section.locator("button").all_inner_texts()
                badges = await advisor_section.locator(".badge, [class*='badge']").all_inner_texts()
                advisor_info["available_buttons"] = [b.strip() for b in buttons if b.strip()]
                advisor_info["model_status_badges"] = [bg.strip() for bg in badges if bg.strip()]

                logger.info(f"  ✓ أزرار التحكم بالموديل المحلي المكتشفة: {advisor_info['available_buttons']}")
                logger.info(f"  ✓ شارات حالة الموديل: {advisor_info['model_status_badges']}")

            shot = await self.capture_silent_screenshot("adv_03_smart_advisor_settings")
            advisor_info["screenshot"] = shot
            self.report.advanced_checks["smart_advisor"] = advisor_info

        except Exception as e:
            logger.error(f"  ❌ خطأ أثناء فحص قسم المستشار الذكي: {e}")
            self.report.advanced_checks["smart_advisor"] = {"error": str(e)}

        # ------------------------------------------------------------------------------
        # 4. محاكاة تعبئة نموذج إضافة تبرع جديد تجريبي (Simulation Entry)
        # ------------------------------------------------------------------------------
        logger.info("\n[4/4] محاكاة فتح وتعبئة نموذج تبرع جديد (/donations)...")
        try:
            await DOMDriver.dispatch_click(page, "a[href='/donations']")
            await asyncio.sleep(1.0)

            # الضغط على زر تسجيل تبرع
            record_btn_selectors = [
                "button[data-shortcut='new']",
                "button:has-text('تسجيل تبرع')",
                "button.btn-primary:has(svg.lucide-plus)"
            ]

            opened_modal = False
            for r_sel in record_btn_selectors:
                if await page.locator(r_sel).count() > 0:
                    await DOMDriver.dispatch_click(page, r_sel)
                    opened_modal = True
                    break

            simulation_info = {"modal_opened": opened_modal, "fields_injected": False}

            if opened_modal:
                await asyncio.sleep(0.8)
                # التحقق من ظهور نافذة الـ Modal
                modal = page.locator(".modal-shell, .modal-card, [role='dialog']").first
                if await modal.count() > 0:
                    logger.info("  ✓ تم فتح نافذة تسجيل التبرع بنجاح، محاكاة حقن القيم...")
                    # حقن مبلغ تجريبي في حقل المبلغ
                    amount_input = modal.locator("input[inputmode='decimal'], input[placeholder*='0.00']").first
                    if await amount_input.count() > 0:
                        await amount_input.fill("500.00")
                        simulation_info["fields_injected"] = True

                    # التقاط صورة توثيقية للنموذج المعبأ
                    shot = await self.capture_silent_screenshot("adv_04_simulated_donation_modal")
                    simulation_info["screenshot"] = shot

                    # إغلاق النافذة برمجياً دون حفظ لتجنب تشويه البيانات الفعلية
                    cancel_btn = modal.locator("button:has-text('إلغاء'), button:has-text('إغلاق')").first
                    if await cancel_btn.count() > 0:
                        await DOMDriver.dispatch_click(page, "button:has-text('إلغاء')")
                        logger.info("  ✓ تم إغلاق النموذج التجريبي بنجاح.")

            self.report.advanced_checks["donation_simulation"] = simulation_info

        except Exception as e:
            logger.warning(f"  ! تنبيه أثناء محاكاة نموذج التبرع: {e}")
            self.report.advanced_checks["donation_simulation"] = {"error": str(e)}

    # ----------------------------------------------------------------------------------
    # السيناريو 4: توليد التقرير الجنائي والملخص الشامل (Forensic Report Generation)
    # ----------------------------------------------------------------------------------

    def generate_final_report(self):
        """توليد التقرير النصي والـ Markdown الشامل لنتائج الاختبار."""
        self.report.end_time = time.strftime("%Y-%m-%d %H:%M:%S")
        report_file = RESULTS_DIR / "summary_report.md"
        json_file = RESULTS_DIR / "audit_log.json"

        # حفظ البيانات الخام بتنسيق JSON
        with open(json_file, "w", encoding="utf-8") as f:
            json.dump({
                "meta": {
                    "start_time": self.report.start_time,
                    "end_time": self.report.end_time,
                    "app_version": self.report.app_version,
                    "cdp_endpoint": self.report.cdp_endpoint,
                    "connected_successfully": self.report.connected_successfully,
                    "boot_error": self.report.boot_error,
                    "login_success": self.report.login_success
                },
                "sections": [s.__dict__ for s in self.report.sections],
                "advanced_checks": self.report.advanced_checks,
                "console_errors": self.report.console_errors,
                "console_warnings": self.report.console_warnings,
                "failed_requests": self.report.failed_requests
            }, f, ensure_ascii=False, indent=2)

        # حساب الإحصائيات العامة
        total_sections = len(self.report.sections)
        passed_sections = sum(1 for s in self.report.sections if s.status == "SUCCESS")
        failed_sections = sum(1 for s in self.report.sections if s.status == "FAILED")
        total_errors = len(self.report.console_errors)
        total_failed_req = len(self.report.failed_requests)

        # صياغة تقرير Markdown احترافي
        md = []
        md.append(f"# 🛡️ تقرير الفحص الجنائي وضمان الجودة - تطبيق يُسر ({self.report.app_version})\n")
        md.append(f"- **تاريخ البدء:** `{self.report.start_time}`")
        md.append(f"- **تاريخ الانتهاء:** `{self.report.end_time}`")
        md.append(f"- **منفذ CDP المستخدم:** `{self.report.cdp_endpoint}`")
        md.append(f"- **حالة الاتصال بـ CDP:** `{'ناجح ✓' if self.report.connected_successfully else 'فشل ❌'}`")
        if self.report.boot_error:
            md.append(f"- **حالة إقلاع قاعدة البيانات (initdb):** `❌ فشل رصد شاشة خطأ ({self.report.boot_error})`")
        else:
            md.append("- **حالة إقلاع قاعدة البيانات (initdb):** `✓ سليمة (لا توجد أخطاء)`")
        md.append(f"- **حالة تسجيل الدخول:** `{'ناجح ✓' if self.report.login_success else 'فشل ❌'}`\n")

        if self.report.boot_error:
            md.append("## 🚨 تنبيه أمني وجنائي: خطأ إقلاع قاعدة البيانات (.boot-error / initdb.exe)")
            md.append("> [!CAUTION]")
            md.append(f"> **تم رصد شاشة خطأ قاعدة البيانات عند الإقلاع:**\n> ```\n> {self.report.boot_error}\n> ```\n")
            boot_shot = self.report.advanced_checks.get("database_boot", {}).get("screenshot")
            if boot_shot:
                md.append(f"> لقطة الشاشة: [{Path(boot_shot).name}]({Path(boot_shot).name})\n")
            md.append("\n")

        md.append("## 📊 ملخص المؤشرات التنفيذية (KPIs)")
        md.append("| المؤشر | القيمة |")
        md.append("| :--- | :--- |")
        md.append(f"| حالة إقلاع قاعدة البيانات | **{'❌ خطأ (.boot-error)' if self.report.boot_error else '✅ سليم'}** |")
        md.append(f"| إجمالي الأقسام المفحوصة | **{total_sections}** |")
        md.append(f"| الأقسام السليمة (Passed) | **{passed_sections}** ✅ |")
        md.append(f"| الأقسام المتعثرة (Failed) | **{failed_sections}** ❌ |")
        md.append(f"| أخطاء الكونسول (Console Errors) | **{total_errors}** |")
        md.append(f"| طلبات الشبكة المتعثرة (Network Failures) | **{total_failed_req}** |\n")

        md.append("## 🧭 نتائج تصفح الأقسام (Sidebar Exploration)")
        md.append("| # | القسم | المسار | الحالة | زمن الاستجابة | لقطة الشاشة | تفاصيل الخطأ |")
        md.append("| :-: | :--- | :--- | :---: | :---: | :--- | :--- |")
        for i, sec in enumerate(self.report.sections, 1):
            status_icon = "✅" if sec.status == "SUCCESS" else "❌"
            shot_link = f"[صورة]({Path(sec.screenshot_path).name})" if sec.screenshot_path else "—"
            err = sec.error_details or "—"
            md.append(f"| {i} | **{sec.name}** | `{sec.path}` | {status_icon} {sec.status} | {sec.duration_sec}s | {shot_link} | {err} |")
        md.append("\n")

        md.append("## 🔬 الفحوصات المتقدمة (Advanced Audits)")
        for key, val in self.report.advanced_checks.items():
            md.append(f"### • فحص `{key}`")
            md.append("```json")
            md.append(json.dumps(val, ensure_ascii=False, indent=2))
            md.append("```\n")

        if self.report.console_errors:
            md.append("## ⚠️ سجل أخطاء الـ Console المكتشفة")
            md.append("| الوقت | رسالة الخطأ | الموقع |")
            md.append("| :--- | :--- | :--- |")
            for err in self.report.console_errors:
                md.append(f"| `{err.get('timestamp')}` | `{err.get('text')}` | `{err.get('location')}` |")
            md.append("\n")

        if self.report.failed_requests:
            md.append("## 🌐 سجل إخفاقات الشبكة (Failed Requests)")
            md.append("| الوقت | الطريقة | الرابط (URL) | سبب الفشل |")
            md.append("| :--- | :---: | :--- | :--- |")
            for req in self.report.failed_requests:
                md.append(f"| `{req.get('timestamp')}` | **{req.get('method')}** | `{req.get('url')}` | `{req.get('failure')}` |")
            md.append("\n")

        with open(report_file, "w", encoding="utf-8") as f:
            f.write("\n".join(md))

        # طباعة الملخص النهائي على الشاشة
        print("\n" + "=" * 70)
        print("🏆 ملخص تقرير الأتمتة والفحص الجنائي لتطبيق يُسر")
        print("=" * 70)
        if self.report.boot_error:
            print(f" ❌ خطأ إقلاع قاعدة البيانات (initdb) : {self.report.boot_error}")
        print(f" الأقسام المفحوصة بنجاح : {passed_sections} من أصل {total_sections}")
        print(f" الأقسام المتعثرة        : {failed_sections}")
        print(f" أخطاء Console المرصودة  : {total_errors}")
        print(f" إخفاقات طلبات الشبكة   : {total_failed_req}")
        print(f" ملف التقرير التفصيلي   : {report_file}")
        print(f" مجلد لقطات الشاشة      : {SCREENSHOTS_DIR}")
        print("=" * 70 + "\n")

    # ----------------------------------------------------------------------------------
    # دورة التنفيذ الكاملة (Complete Execution Pipeline)
    # ----------------------------------------------------------------------------------

    async def run(self):
        """تنفيذ سلسلة الاختبارات المتكاملة بالترتيب."""
        self.report.start_time = time.strftime("%Y-%m-%d %H:%M:%S")

        # 1. التحقق من تشغيل التطبيق وجاهزية منفذ CDP
        is_ready = await YusrAppLauncher.ensure_app_running(max_wait_sec=25)
        if not is_ready:
            logger.error("❌ توقف السكربت نظراً لتعذر الوصول إلى جلسة التطبيق على المنفذ 9222.")
            self.generate_final_report()
            return

        # 2. الاتصال بـ WebView2 عبر Playwright
        connected = await self.setup_connection()
        if not connected:
            self.generate_final_report()
            return

        try:
            # 3. اختبار تسجيل الدخول الذكي
            login_ok = await self.execute_smart_login()
            if not login_ok:
                logger.warning("! تعثر تسجيل الدخول، ستتم محاولة مواصلة الفحص بالمسارات المباشرة...")

            # 4. الاستكشاف الديناميكي لأقسام القائمة الجانبية
            await self.explore_sidebar_sections()

            # 5. فحص الميزات الحساسة والمتقدمة
            await self.execute_advanced_checks()

        finally:
            # 6. توليد التقارير وإنهاء الاتصال بنظافة
            self.generate_final_report()
            if self.browser:
                await self.browser.close()
            if self.playwright:
                await self.playwright.stop()
            logger.info("✓ تم إنهاء جلسة الفحص والأتمتة بنجاح.")


# --------------------------------------------------------------------------------------
# 5. نقطة الدخول الرئيسية للسكربت (Main Entry Point)
# --------------------------------------------------------------------------------------

if __name__ == "__main__":
    # تشغيل الحلقة غير المتزامنة
    asyncio.run(YusrAutomationSuite().run())

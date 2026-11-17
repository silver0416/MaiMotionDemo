// AppImage 影片播放檢查（release.yml 的「Check AppImage」用，見 check.sh）。
//
// 檢查時用這支程式取代 AppImage 裡的主程式，再經由 AppRun 執行，所以用的是 AppImage 內附的
// WebKitGTK、GStreamer 與外掛，環境變數和工作目錄也和 App 相同。
// 它照 wry／Tauri 在 Linux 上的做法註冊 tauri:// 與 asset:// 兩個 scheme（asset:// 的回應照
// Tauri 的 asset protocol：Range 請求回 206、每次最多 1000 KiB），另外開一個 127.0.0.1 的 http
// 伺服器提供同樣的影片，載入 media.html。頁面用和 App 相同的方式播放、跳轉、逐格讀取測試影片，
// 結果寫在頁面標題上，這裡印出來。
//
// 用法：media-check <資料夾>（裡面有 media.html 與測試影片）
// 結束碼：0 全部通過、1 有必要項目失敗、2 頁面載入失敗、3 逾時、4 WebKit 的畫面程序中止

#include <webkit2/webkit2.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define WINDOW_ORIGIN "tauri://localhost"
#define ASSET_PREFIX "asset://localhost/"
#define MAX_LEN ((guint64)1000 * 1024)

static const char *media_dir;
static int exit_code = 3;

static const char *content_type_for(const char *path)
{
    if (g_str_has_suffix(path, ".html"))
        return "text/html";
    if (g_str_has_suffix(path, ".mp4"))
        return "video/mp4";
    if (g_str_has_suffix(path, ".webm"))
        return "video/webm";
    return "application/octet-stream";
}

// 和 wry 一樣：內容放進記憶體串流，狀態碼、Content-Type 與標頭照抄。headers 交給 WebKit。
static void respond(WebKitURISchemeRequest *request, guint status, const char *type, GBytes *body,
                    SoupMessageHeaders *headers)
{
    GInputStream *stream = g_memory_input_stream_new_from_bytes(body);
    WebKitURISchemeResponse *response = webkit_uri_scheme_response_new(stream, g_bytes_get_size(body));
    webkit_uri_scheme_response_set_status(response, status, NULL);
    if (type)
        webkit_uri_scheme_response_set_content_type(response, type);
    webkit_uri_scheme_response_set_http_headers(response, headers);
    webkit_uri_scheme_request_finish_with_response(request, response);
    g_object_unref(response);
    g_object_unref(stream);
}

static void respond_empty(WebKitURISchemeRequest *request, guint status, SoupMessageHeaders *headers)
{
    GBytes *empty = g_bytes_new(NULL, 0);
    respond(request, status, NULL, empty, headers);
    g_bytes_unref(empty);
}

// tauri://localhost/<檔名>：從資料夾讀 media.html
static void serve_page(WebKitURISchemeRequest *request, gpointer user_data)
{
    (void)user_data;
    char *file = g_build_filename(media_dir, webkit_uri_scheme_request_get_path(request), NULL);
    gchar *contents = NULL;
    gsize length = 0;
    GError *error = NULL;
    if (!g_file_get_contents(file, &contents, &length, &error)) {
        webkit_uri_scheme_request_finish_error(request, error);
        g_error_free(error);
        g_free(file);
        return;
    }
    GBytes *body = g_bytes_new_take(contents, length);
    SoupMessageHeaders *headers = soup_message_headers_new(SOUP_MESSAGE_HEADERS_RESPONSE);
    soup_message_headers_append(headers, "Content-Type", content_type_for(file));
    respond(request, 200, content_type_for(file), body, headers);
    g_bytes_unref(body);
    g_free(file);
}

// 和 Tauri 用的 http-range 一樣解析單一範圍：bytes=a-b、bytes=a-、bytes=-n
static gboolean parse_range(const char *header, guint64 len, guint64 *start, guint64 *end)
{
    if (!g_str_has_prefix(header, "bytes=") || strchr(header, ','))
        return FALSE;
    const char *spec = header + 6;
    const char *dash = strchr(spec, '-');
    if (!dash || len == 0)
        return FALSE;
    if (dash == spec) {
        guint64 suffix = g_ascii_strtoull(dash + 1, NULL, 10);
        if (suffix == 0)
            return FALSE;
        *start = suffix >= len ? 0 : len - suffix;
        *end = len - 1;
    } else {
        *start = g_ascii_strtoull(spec, NULL, 10);
        *end = dash[1] ? g_ascii_strtoull(dash + 1, NULL, 10) : len - 1;
        if (*end >= len)
            *end = len - 1;
    }
    return *start < len && *start <= *end;
}

// asset://localhost/<百分比編碼的完整路徑>：照 Tauri 2.11 的 asset protocol 回應
static void serve_asset(WebKitURISchemeRequest *request, gpointer user_data)
{
    (void)user_data;
    const char *uri = webkit_uri_scheme_request_get_uri(request);
    char *encoded = g_strdup(g_str_has_prefix(uri, ASSET_PREFIX) ? uri + strlen(ASSET_PREFIX) : "");
    encoded[strcspn(encoded, "?#")] = '\0';
    char *path = g_uri_unescape_string(encoded, NULL);
    g_free(encoded);

    SoupMessageHeaders *headers = soup_message_headers_new(SOUP_MESSAGE_HEADERS_RESPONSE);
    soup_message_headers_append(headers, "Access-Control-Allow-Origin", WINDOW_ORIGIN);
    GMappedFile *mapped = path ? g_mapped_file_new(path, FALSE, NULL) : NULL;
    if (!mapped) {
        respond_empty(request, 404, headers);
        g_free(path);
        return;
    }
    GBytes *all = g_mapped_file_get_bytes(mapped);
    guint64 len = g_bytes_get_size(all);
    const char *type = content_type_for(path);
    soup_message_headers_append(headers, "Content-Type", type);

    SoupMessageHeaders *request_headers = webkit_uri_scheme_request_get_http_headers(request);
    const char *range = request_headers ? soup_message_headers_get_one(request_headers, "Range") : NULL;
    const char *method = webkit_uri_scheme_request_get_http_method(request);
    char value[96];
    if (range) {
        guint64 start, end;
        if (!parse_range(range, len, &start, &end)) {
            soup_message_headers_unref(headers);
            SoupMessageHeaders *invalid = soup_message_headers_new(SOUP_MESSAGE_HEADERS_RESPONSE);
            g_snprintf(value, sizeof value, "bytes */%" G_GUINT64_FORMAT, len);
            soup_message_headers_append(invalid, "Content-Range", value);
            respond_empty(request, 416, invalid);
        } else {
            if (end - start + 1 > MAX_LEN)
                end = start + MAX_LEN - 1;
            soup_message_headers_append(headers, "Accept-Ranges", "bytes");
            soup_message_headers_append(headers, "Access-Control-Expose-Headers", "content-range");
            g_snprintf(value, sizeof value, "bytes %" G_GUINT64_FORMAT "-%" G_GUINT64_FORMAT "/%" G_GUINT64_FORMAT,
                       start, end, len);
            soup_message_headers_append(headers, "Content-Range", value);
            g_snprintf(value, sizeof value, "%" G_GUINT64_FORMAT, end - start + 1);
            soup_message_headers_append(headers, "Content-Length", value);
            GBytes *part = g_bytes_new_from_bytes(all, start, end - start + 1);
            respond(request, 206, type, part, headers);
            g_bytes_unref(part);
        }
    } else {
        g_snprintf(value, sizeof value, "%" G_GUINT64_FORMAT, len);
        soup_message_headers_append(headers, "Content-Length", value);
        if (method && strcmp(method, "HEAD") == 0)
            respond_empty(request, 200, headers);
        else
            respond(request, 200, type, all, headers);
    }
    g_bytes_unref(all);
    g_mapped_file_unref(mapped);
    g_free(path);
}

// http://127.0.0.1:<port>/<檔名>：從資料夾讀測試影片，支援 Range，允許跨來源讀取
static void serve_http(SoupServer *server, SoupServerMessage *msg, const char *path, GHashTable *query,
                       gpointer user_data)
{
    (void)server;
    (void)query;
    (void)user_data;
    SoupMessageHeaders *headers = soup_server_message_get_response_headers(msg);
    soup_message_headers_append(headers, "Access-Control-Allow-Origin", "*");
    char *file = g_build_filename(media_dir, path, NULL);
    GMappedFile *mapped = g_mapped_file_new(file, FALSE, NULL);
    g_free(file);
    if (!mapped) {
        soup_server_message_set_status(msg, 404, NULL);
        return;
    }
    gsize len = 0;
    GBytes *all = g_mapped_file_get_bytes(mapped);
    const char *data = g_bytes_get_data(all, &len);
    soup_message_headers_append(headers, "Accept-Ranges", "bytes");
    const char *range = soup_message_headers_get_one(soup_server_message_get_request_headers(msg), "Range");
    guint64 start = 0, end = len ? len - 1 : 0;
    guint status = 200;
    char value[96];
    if (range) {
        if (!parse_range(range, len, &start, &end)) {
            g_snprintf(value, sizeof value, "bytes */%" G_GSIZE_FORMAT, len);
            soup_message_headers_append(headers, "Content-Range", value);
            soup_server_message_set_status(msg, 416, NULL);
            g_bytes_unref(all);
            g_mapped_file_unref(mapped);
            return;
        }
        status = 206;
        g_snprintf(value, sizeof value, "bytes %" G_GUINT64_FORMAT "-%" G_GUINT64_FORMAT "/%" G_GSIZE_FORMAT, start, end,
                   len);
        soup_message_headers_append(headers, "Content-Range", value);
    }
    soup_server_message_set_status(msg, status, NULL);
    soup_server_message_set_response(msg, content_type_for(path), SOUP_MEMORY_COPY, data + start,
                                     len ? end - start + 1 : 0);
    g_bytes_unref(all);
    g_mapped_file_unref(mapped);
}

// 頁面用標題回報：「LOG <序號> <內容>」一行一筆，「DONE pass」或「DONE fail」結束
static void on_title(WebKitWebView *view, GParamSpec *pspec, gpointer user_data)
{
    (void)pspec;
    (void)user_data;
    const char *title = webkit_web_view_get_title(view);
    if (!title)
        return;
    if (g_str_has_prefix(title, "LOG ")) {
        const char *text = strchr(title + 4, ' ');
        printf("%s\n", text ? text + 1 : title);
        fflush(stdout);
    } else if (g_str_has_prefix(title, "DONE ")) {
        exit_code = strcmp(title + 5, "pass") == 0 ? 0 : 1;
        gtk_main_quit();
    }
}

static gboolean on_load_failed(WebKitWebView *view, WebKitLoadEvent event, gchar *uri, GError *error,
                               gpointer user_data)
{
    (void)view;
    (void)event;
    (void)user_data;
    fprintf(stderr, "media-check: failed to load %s: %s\n", uri, error->message);
    exit_code = 2;
    gtk_main_quit();
    return TRUE;
}

static void on_terminated(WebKitWebView *view, WebKitWebProcessTerminationReason reason, gpointer user_data)
{
    (void)view;
    (void)user_data;
    fprintf(stderr, "media-check: web process terminated (reason %d)\n", reason);
    exit_code = 4;
    gtk_main_quit();
}

static gboolean on_timeout(gpointer user_data)
{
    (void)user_data;
    fprintf(stderr, "media-check: timed out\n");
    exit_code = 3;
    gtk_main_quit();
    return G_SOURCE_REMOVE;
}

int main(int argc, char **argv)
{
    gtk_init(&argc, &argv);
    if (argc < 2) {
        fprintf(stderr, "usage: %s <media dir>\n", argv[0]);
        return 2;
    }
    media_dir = argv[1];
    printf("WebKitGTK %u.%u.%u\n", webkit_get_major_version(), webkit_get_minor_version(),
           webkit_get_micro_version());
    printf("GST_PLUGIN_SYSTEM_PATH_1_0=%s\n", g_getenv("GST_PLUGIN_SYSTEM_PATH_1_0"));
    printf("GST_PLUGIN_SCANNER_1_0=%s\n", g_getenv("GST_PLUGIN_SCANNER_1_0"));
    fflush(stdout);

    // 和 wry 一樣：每個自訂 scheme 都登記成安全來源（WebCodecs 只在安全環境提供）
    WebKitWebContext *context = webkit_web_context_get_default();
    WebKitSecurityManager *security = webkit_web_context_get_security_manager(context);
    webkit_security_manager_register_uri_scheme_as_secure(security, "tauri");
    webkit_web_context_register_uri_scheme(context, "tauri", serve_page, NULL, NULL);
    webkit_security_manager_register_uri_scheme_as_secure(security, "asset");
    webkit_web_context_register_uri_scheme(context, "asset", serve_asset, NULL, NULL);

    WebKitWebView *view = WEBKIT_WEB_VIEW(webkit_web_view_new());
    WebKitSettings *settings = webkit_web_view_get_settings(view);
    webkit_settings_set_enable_webgl(settings, TRUE);
    webkit_settings_set_enable_webaudio(settings, TRUE);
    g_signal_connect(view, "notify::title", G_CALLBACK(on_title), NULL);
    g_signal_connect(view, "load-failed", G_CALLBACK(on_load_failed), NULL);
    g_signal_connect(view, "web-process-terminated", G_CALLBACK(on_terminated), NULL);

    GtkWidget *window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
    gtk_window_set_default_size(GTK_WINDOW(window), 800, 600);
    gtk_container_add(GTK_CONTAINER(window), GTK_WIDGET(view));
    gtk_widget_show_all(window);

    // 本機 http 伺服器：比較用 http://127.0.0.1 提供影片時能不能播
    SoupServer *server = soup_server_new(NULL, NULL);
    soup_server_add_handler(server, NULL, serve_http, NULL, NULL);
    GError *error = NULL;
    if (!soup_server_listen_local(server, 0, SOUP_SERVER_LISTEN_IPV4_ONLY, &error)) {
        fprintf(stderr, "media-check: cannot start the http server: %s\n", error->message);
        return 2;
    }
    GSList *uris = soup_server_get_uris(server);
    int port = uris ? g_uri_get_port(uris->data) : 0;
    g_slist_free_full(uris, (GDestroyNotify)g_uri_unref);

    char *dir = g_uri_escape_string(media_dir, NULL, FALSE);
    char *url = g_strdup_printf(WINDOW_ORIGIN "/media.html?dir=%s&http=%d", dir, port);
    webkit_web_view_load_uri(view, url);
    g_free(url);
    g_free(dir);

    g_timeout_add_seconds(180, on_timeout, NULL);
    gtk_main();
    return exit_code;
}

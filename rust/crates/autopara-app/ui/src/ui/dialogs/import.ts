// Importing a timetable (Python `ui/setup_dialog.py`): choose the file (drop it, or browse), then
// the course and the group. Opens on the previously chosen course and group when the new document
// still has them, so re-importing an updated timetable is two clicks.
import type { App } from "../../app";
import type { CoursePreview } from "../../bindings/CoursePreview";
import { h } from "../../dom";
import { t } from "../../i18n";
import { DropWell } from "../landing";
import { inform, modal } from "../popups";

export function openImport(app: App, initialPath?: string): Promise<boolean | null> {
  const chosen = app.settings.selected_group_id;
  const course = chosen ? app.courseOf(chosen) : undefined;
  const remembered = { ordinal: course?.course.ordinal, group: course?.groups.find((g) => g.group.id === chosen)?.group.name };

  return modal<boolean>((finish) => {
    let path: string | null = null;
    let courses: CoursePreview[] = [];
    let groupName: string | null = null;
    const courseSelect = h("select", { class: "select", "aria-label": t("import_course"), onchange: () => showGroups() });
    const groupList = h("div", { class: "list", role: "listbox", "aria-label": t("import_group") });
    const summary = h("p", { class: "hint", role: "status" });
    const importButton = h("button", { class: "btn primary", disabled: true, onclick: () => void accept() }, t("import_button"));
    const well = new DropWell(() => void browse(), true);

    const selectedCourse = () => courses.find((c) => String(c.ordinal) === courseSelect.value);

    const showSummary = () => {
      const group = selectedCourse()?.groups.find((g) => g.name === groupName);
      importButton.disabled = !group;
      if (!group) return void (summary.textContent = "");
      if (!group.lessons) return void (summary.textContent = t("import_summary_none"));
      const parts = [t("import_summary_pairs", { count: String(group.lessons) })];
      if (group.shared) parts.push(t("import_summary_shared", { count: String(group.shared) }));
      if (group.without_link) parts.push(t("import_summary_nolink", { count: String(group.without_link) }));
      summary.textContent = parts.join(" · ");
    };

    const showGroups = () => {
      const course = selectedCourse();
      const names = course?.groups.map((g) => g.name) ?? [];
      groupName = names.includes(remembered.group ?? "") ? (remembered.group ?? null) : (names[0] ?? null);
      groupList.replaceChildren(
        ...(course?.groups ?? []).map((group) =>
          h(
            "button",
            {
              role: "option",
              "aria-selected": String(group.name === groupName),
              onclick: (event) => {
                groupName = group.name;
                for (const option of groupList.children) option.setAttribute("aria-selected", String(option === event.currentTarget));
                showSummary();
              },
            },
            t("import_group_item", { group: group.name, specialty: group.specialty, count: String(group.lessons) }),
          ),
        ),
      );
      showSummary();
    };

    const load = async (file: string) => {
      path = file;
      well.showFile(file);
      try {
        courses = await app.backend.previewTimetable(file);
      } catch (error) {
        courses = [];
        importButton.disabled = true;
        return inform(t("import_read_failed"), `${file}\n\n${String(error)}`);
      }
      if (!courses.length) return inform(t("import_no_tables_title"), t("import_no_tables"));
      courseSelect.replaceChildren(
        ...courses.map((c) =>
          h("option", { value: String(c.ordinal) }, t("import_course_item", { course: c.name, count: c.lessons ? t("import_pairs", { count: String(c.lessons) }) : t("import_no_pairs") })),
        ),
      );
      if (remembered.ordinal && courses.some((c) => c.ordinal === remembered.ordinal)) courseSelect.value = String(remembered.ordinal);
      showGroups();
    };

    const browse = async () => {
      const file = await app.backend.pickTimetable();
      if (file) await load(file);
    };

    const accept = async () => {
      const course = selectedCourse();
      if (!path || !course || !groupName) return;
      try {
        await app.backend.importTimetable(path, course.ordinal, groupName);
        finish(true);
      } catch (error) {
        await inform(t("import_failed"), String(error));
      }
    };

    app.backend.onFileDrop((kind, paths) => {
      const docx = paths.find((p) => p.toLowerCase().endsWith(".docx"));
      well.highlight(kind === "hover" && Boolean(docx));
      if (kind === "drop" && docx) void load(docx);
    });
    const start = initialPath ?? app.settings.schedule_copy_path ?? app.settings.last_import_path;
    if (start) void load(start);

    return h(
      "div",
      { class: "dialog import" },
      h("h2", {}, t("import_title")),
      h("p", { class: "hint" }, t("import_hint")),
      well.element,
      h("div", { class: "section" }, t("import_course")),
      courseSelect,
      h("div", { class: "section" }, t("import_group")),
      groupList,
      summary,
      h("div", { class: "footer" }, h("button", { class: "btn plain", onclick: () => finish(false) }, t("cancel")), importButton),
    );
  }, t("import_title"));
}

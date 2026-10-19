import type { CourseDownloadStatus } from "../gen/proto/admin_pb";

export function learningProgress(course?: CourseDownloadStatus) {
	const items =
		course?.modules.flatMap((module) =>
			module.lessons.flatMap((lesson) => lesson.items),
		) ?? [];
	const completed = items.filter((item) => item.completed).length;
	return {
		total: items.length,
		completed,
		percent: items.length ? Math.round((completed / items.length) * 100) : 0,
	};
}

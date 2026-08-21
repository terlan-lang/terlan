function contains_score(value, term, weight) {
	return value.includes(term) ? weight : 0;
}
export function score_term(title, description, section, content, term) {
	return title_score(title, term) + contains_score(section, term, 8) + contains_score(description, term, 5) + contains_score(content, term, 1);
}
function title_score(title, term) {
	return title === term ? 40 : title.includes(term) ? 20 : 0;
}

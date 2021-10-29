use crate::template::Template;

#[derive(Debug, Clone)]
pub struct FindResult<'a> {
    pub name_and_description: Vec<&'a Template>,
    pub name_only: Vec<&'a Template>,
    pub description_only: Vec<&'a Template>,
}

impl<'a> FindResult<'a> {
    pub fn compose(&self, name: &'a str) -> FindResultComposite<'a> {
        FindResultComposite {
            name_and_description: self
                .name_and_description
                .iter()
                .map(|&it| (name, it))
                .collect(),
            name_only: self.name_only.iter().map(|&it| (name, it)).collect(),
            description_only: self.description_only.iter().map(|&it| (name, it)).collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FindResultComposite<'a> {
    pub name_and_description: Vec<(&'a str, &'a Template)>,
    pub name_only: Vec<(&'a str, &'a Template)>,
    pub description_only: Vec<(&'a str, &'a Template)>,
}

impl<'a> FindResultComposite<'a> {
    pub fn merge_ref<'b>(&mut self, other: FindResultComposite<'b>)
    where
        'a: 'b,
        'b: 'a,
    {
        self.name_and_description.extend(other.name_and_description);
        self.name_only.extend(other.name_only);
        self.description_only.extend(other.description_only);
    }

    pub fn merge(mut self, other: Self) -> Self {
        self.merge_ref(other);
        self
    }
}

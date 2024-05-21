use activitypub_federation::{fetch::object_id::ObjectId, http_signatures::generate_actor_keypair};
use activitystreams_kinds::public;
use chrono::{DateTime, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use anyhow::anyhow;
use url::Url;

use crate::{database::State, entities::{self, post, prelude, user}, objects::post::Mention, utils::{generate_object_id, generate_user_id}, API_DOMAIN, DB, FEDERATION_CONFIG, LYSAND_DOMAIN};

use super::{objects::{ContentFormat, Note}, superx::request_client};

pub async fn fetch_user_from_url(url: Url) -> anyhow::Result<super::objects::User> {
    let req_client = request_client();
    let request = req_client.get(url).send().await?;
    Ok(request.json::<super::objects::User>().await?)
}

pub async fn option_content_format_text(opt: Option<ContentFormat>) -> Option<String> {
    if let Some(format) = opt {
        return Some(format.select_rich_text().await.unwrap());
    }

    None
}

pub async fn db_user_from_url(url: Url) -> anyhow::Result<entities::user::Model> {
    if !url.domain().eq(&Some(LYSAND_DOMAIN.as_str())) {
        return Err(anyhow!("not lysands domain"));
    }
    let user_res = prelude::User::find().filter(entities::user::Column::Url.eq(url.to_string())).one(DB.get().unwrap()).await?;

    if let Some(user) = user_res {
        Ok(user)
    } else {
        let ls_user = fetch_user_from_url(url).await?;
        let keypair = generate_actor_keypair()?;
        let user = entities::user::ActiveModel {
            id: Set(ls_user.id.to_string()),
            username: Set(ls_user.username.clone()),
            name: Set(ls_user.display_name.unwrap_or(ls_user.username)),
            inbox: Set(ls_user.inbox.to_string()),
            public_key: Set(keypair.public_key.clone()),
            private_key: Set(Some(keypair.private_key.clone())),
            last_refreshed_at: Set(Utc::now()),
            follower_count: Set(0),
            following_count: Set(0),
            url: Set(ls_user.uri.to_string()),
            local: Set(true),
            created_at: Set(DateTime::from_timestamp(ls_user.created_at.unix_timestamp(), 0).unwrap()),
            summary: Set(option_content_format_text(ls_user.bio).await),
            updated_at: Set(Some(Utc::now())),
            followers: Set(Some(ls_user.followers.to_string())),
            following: Set(Some(ls_user.following.to_string())),
            ..Default::default()
        };
        let db = DB.get().unwrap();
        Ok(user.insert(db).await?)
    }
}

pub async fn fetch_note_from_url(url: Url) -> anyhow::Result<super::objects::Note> {
    let req_client = request_client();
    let request = req_client.get(url).send().await?;
    Ok(request.json::<super::objects::Note>().await?)
}

pub async fn receive_lysand_note(note: Note, db_id: String) -> anyhow::Result<()> {
    let author: entities::user::Model = db_user_from_url(note.author.clone()).await?;
    let user_res = prelude::User::find_by_id(db_id).one(DB.get().unwrap()).await;
    if user_res.is_err() {
        println!("{}", user_res.as_ref().unwrap_err());
        return Err(user_res.err().unwrap().into());
    }
    if let Some(target) = user_res? {
        let data = FEDERATION_CONFIG.get().unwrap();
        let id: ObjectId<post::Model> = generate_object_id(data.domain(), &note.id.to_string())?.into();
        let user_id = generate_user_id(data.domain(), &target.id.to_string())?;
        let user = fetch_user_from_url(user_id).await?;
        let mut tag: Vec<Mention> = Vec::new();
        for l_tag in note.mentions.clone().unwrap_or_default() {
            tag.push(Mention { href: l_tag, //todo convert to ap url
                kind: Default::default(), })
        }
        let to = match note.visibility.clone().unwrap_or(super::objects::VisibilityType::Public) {
            super::objects::VisibilityType::Public => vec![public(), Url::parse(&author.followers.unwrap_or_default())?],
            super::objects::VisibilityType::Followers => vec![Url::parse(&author.followers.unwrap_or_default())?],
            super::objects::VisibilityType::Direct => note.mentions.unwrap_or_default(),
            super::objects::VisibilityType::Unlisted => vec![Url::parse(&author.followers.unwrap_or_default())?],
        };
        let cc = match note.visibility.unwrap_or(super::objects::VisibilityType::Public) {
            super::objects::VisibilityType::Unlisted => Some(vec![public()]),
            _ => None
        };
        let reply: Option<ObjectId<entities::post::Model>> = if let Some(rep) = note.replies_to {
            let note = fetch_note_from_url(rep).await?;
            let fake_rep_url = Url::parse(&format!(
                "https://{}/lysand/apnote/{}",
                API_DOMAIN.to_string(),
                &note.id.to_string()
            ))?;
            Some(fake_rep_url.into())
        } else {
            None
        };
        let ap_note = crate::objects::post::Note {
            kind: Default::default(),
            id,
            sensitive: note.is_sensitive.unwrap_or(false),
            cc,
            to,
            tag,
            attributed_to: Url::parse(author.url.clone().as_str()).unwrap().into(),
            content: option_content_format_text(note.content).await.unwrap_or_default(),
            in_reply_to: reply
        };
    }


    Ok(())
}